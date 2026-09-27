// 使用真实 Wasm 和宿主桥验证输入状态；内存夹具不代表后端任务或真实模型验收。
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { startConversationPreview } from "./preview-conversation-ui.mjs";

const directory = "test-results/conversation-processing";
const providerId = "11111111-1111-4111-8111-111111111111";
const spaceId = "processing-regression";
const spaces = [
  {
    id: spaceId,
    title: "状态回归空间",
    personal: true,
    role: "OWNER",
    modelBinding: providerId,
  },
];
const viewports = [
  { name: "desktop", width: 1440, height: 900 },
  { name: "mobile", width: 390, height: 844 },
];

function message(role, status, memoryStatus, content) {
  return {
    id: randomUUID(),
    role,
    content,
    status,
    memoryStatus,
    error: null,
    tokens: null,
    sourceId: null,
    citations: [],
    route: null,
    matchedNodeIds: [],
    activatedNodeIds: [],
  };
}

function fixture(scenario) {
  const assistant = message(
    "assistant",
    scenario.status,
    scenario.memoryStatus,
    scenario.memoryStatus === "unavailable"
      ? "[来源已删除或当前不可访问]"
      : "这是输入状态回归使用的已有回复。",
  );
  const messages = [
    message("user", "complete", "complete", "继续处理这个任务。"),
  ];
  if (scenario.historyPending) {
    messages.unshift(
      message("assistant", "complete", "pending", "较早的资料仍在后台整理。"),
    );
  }
  messages.push(assistant);
  const pendingInput =
    scenario.status === "awaiting_input"
      ? {
          id: randomUUID(),
          assistantId: assistant.id,
          questions: [
            {
              id: "choice",
              title: "选择执行范围",
              options: [],
              allowText: true,
            },
          ],
        }
      : null;
  return {
    conversation: {
      id: randomUUID(),
      title: `输入状态验收 ${scenario.name}`,
      providerId,
      model: "gpt-6",
      spaceId,
      workerId: null,
      workspaceId: null,
      updatedAt: new Date().toISOString(),
    },
    messages,
    pendingInput,
  };
}

const terminalScenarios = [
  "complete",
  "failed",
  "cancelled",
  "interrupted",
].flatMap((status) =>
  ["pending", "processing"].map((memoryStatus) => ({
    name: `${status}-${memoryStatus}`,
    status,
    memoryStatus,
    canSend: true,
  })),
);
const scenarios = [
  ...terminalScenarios,
  {
    name: "history-pending-last-failed-unavailable",
    status: "failed",
    memoryStatus: "unavailable",
    historyPending: true,
    canSend: true,
  },
  {
    name: "generating",
    status: "generating",
    memoryStatus: "complete",
    canSend: false,
  },
  {
    name: "queued-unavailable",
    status: "queued",
    memoryStatus: "unavailable",
    canSend: false,
  },
  {
    name: "awaiting-input",
    status: "awaiting_input",
    memoryStatus: "complete",
    canSend: false,
  },
];

async function eventually(predicate, description) {
  for (let attempt = 0; attempt < 100; attempt++) {
    if (await predicate()) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  assert.fail(description);
}

async function geometry(frame, label) {
  const metrics = await frame
    .locator(".dx-conversation__composer")
    .evaluate((element) => {
      const rect = element.getBoundingClientRect();
      return {
        width: innerWidth,
        height: innerHeight,
        scrollWidth: document.documentElement.scrollWidth,
        composer: rect.toJSON(),
        input: element
          .querySelector("textarea")
          .getBoundingClientRect()
          .toJSON(),
      };
    });
  assert(metrics.scrollWidth <= metrics.width, `${label}: 页面横向溢出`);
  assert(
    metrics.composer.x >= 0 && metrics.composer.right <= metrics.width,
    `${label}: 输入区横向溢出`,
  );
  assert(
    metrics.composer.y >= 0 && metrics.composer.bottom <= metrics.height,
    `${label}: 输入区不可见`,
  );
  assert(
    metrics.input.width > 0 && metrics.input.height > 0,
    `${label}: 输入框为空`,
  );
  return metrics;
}

await mkdir(directory, { recursive: true });
const browser = await chromium.launch({
  channel: process.env.AIO_UI_BROWSER_CHANNEL,
});
const results = [];
const errors = [];

async function verifyScenario(viewport, scenario) {
  const label = `${viewport.name}-${scenario.name}`;
  const thread = fixture(scenario);
  const { server, origin, calls } = await startConversationPreview(0, {
    threads: [thread],
    spaces,
  });
  const context = await browser.newContext({
    viewport: { width: viewport.width, height: viewport.height },
    colorScheme: "light",
  });
  const page = await context.newPage();
  const caseErrors = [];
  page.on("pageerror", (error) => caseErrors.push(error.message));
  page.on("console", (item) => {
    if (item.type() === "error") {
      caseErrors.push(item.text());
    }
  });
  page.on("response", (response) => {
    if (response.status() >= 400) {
      caseErrors.push(`HTTP ${response.status()} ${response.url()}`);
    }
  });
  try {
    await page.goto(origin);
    const frame = page.frameLocator("iframe");
    await frame
      .getByText(thread.messages.at(-1).content, { exact: true })
      .waitFor();
    const input = frame.getByRole("textbox", { name: "发送消息", exact: true });
    await input.waitFor();
    if (scenario.status === "awaiting_input") {
      await frame
        .getByRole("button", { name: "稍后回答", exact: true })
        .click();
    }
    assert(await input.isEditable(), `${label}: 输入框必须可编辑`);
    const draft = `保留下一条草稿 ${scenario.name}`;
    await input.fill(draft);
    assert.equal(await input.inputValue(), draft, `${label}: 草稿未写入`);
    const expectedStatus =
      {
        generating: "正在生成回复…",
        queued: "正在保存消息…",
        awaiting_input: "等待回答后继续",
      }[scenario.status] ?? "正在整理记忆…";
    assert.equal(
      await frame
        .locator(".dx-conversation__composer-footer [role=status]")
        .textContent(),
      expectedStatus,
      `${label}: 前台任务与后台记忆提示必须区分`,
    );
    const messageCalls = () =>
      calls.filter(
        (call) => call.method === "POST" && call.path.endsWith("/messages"),
      );
    assert.equal(messageCalls().length, 0, `${label}: 编辑草稿不得发送`);
    const measurements = await geometry(frame, label);
    await page.screenshot({ path: `${directory}/${label}.png` });

    if (scenario.canSend) {
      const send = frame.getByRole("button", { name: "发送", exact: true });
      assert(await send.isEnabled(), `${label}: 终态历史不得阻止发送`);
      await eventually(
        () =>
          calls.filter(
            (call) =>
              call.method === "GET" &&
              call.path === `/conversations/${thread.conversation.id}`,
          ).length >= 2,
        `${label}: 后台记忆状态仍应轮询`,
      );
      assert.equal(
        await input.inputValue(),
        draft,
        `${label}: 后台轮询不得丢失草稿`,
      );
      await send.click();
      await eventually(
        () => messageCalls().length === 1,
        `${label}: 新消息未到达宿主桥`,
      );
      assert.equal(
        messageCalls()[0].body.content,
        draft,
        `${label}: 发送内容与草稿不一致`,
      );
    } else {
      await input.press("Enter");
      await page.waitForTimeout(800);
      assert.equal(
        messageCalls().length,
        0,
        `${label}: 未完成任务期间 Enter 不得发送`,
      );
      assert.equal(
        await input.inputValue(),
        draft,
        `${label}: 被阻止的发送不得清空草稿`,
      );
      if (scenario.status === "generating") {
        await frame.getByRole("button", { name: "停止", exact: true }).click();
        await eventually(
          () => calls.some((call) => call.path.endsWith("/cancel")),
          `${label}: 未调用停止接口`,
        );
        const send = frame.getByRole("button", { name: "发送", exact: true });
        await eventually(() => send.isEnabled(), `${label}: 停止后发送未恢复`);
        assert.equal(
          await input.inputValue(),
          draft,
          `${label}: 停止后草稿丢失`,
        );
        await page.screenshot({ path: `${directory}/${label}-stopped.png` });
        await input.press("Enter");
        await eventually(
          () => messageCalls().length === 1,
          `${label}: 停止后草稿无法发送`,
        );
        assert.equal(messageCalls()[0].body.content, draft);
      } else {
        assert(
          await frame
            .getByRole("button", { name: "发送", exact: true })
            .isDisabled(),
          `${label}: 未完成任务仍须保护发送`,
        );
      }
    }
    assert.deepEqual(caseErrors, [], `${label}: 浏览器错误`);
    results.push({
      label,
      passed: true,
      measurements,
      sent: messageCalls().length,
    });
  } catch (error) {
    await page.screenshot({ path: `${directory}/${label}-failure.png` });
    errors.push({ label, error: error.message, browserErrors: caseErrors });
    throw error;
  } finally {
    await context.close();
    await new Promise((resolve, reject) =>
      server.close((error) => (error ? reject(error) : resolve())),
    );
  }
}

try {
  for (const viewport of viewports) {
    for (const scenario of scenarios) {
      await verifyScenario(viewport, scenario);
    }
  }
  console.log(
    `PASS: ${results.length} desktop/mobile input-state scenarios, memory polling, editable drafts, send guards and stop recovery`,
  );
} finally {
  await writeFile(
    `${directory}/report.json`,
    JSON.stringify({ results, errors }, null, 2),
  );
  await browser.close();
}
