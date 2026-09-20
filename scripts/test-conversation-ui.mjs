// 验证编译产物的布局与业务接线，不把内存夹具当作真实模型或服务端验收。
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { startConversationPreview } from "./preview-conversation-ui.mjs";

const directory = "test-results/conversation-ui";
await mkdir(directory, { recursive: true });
const { server, origin, calls } = await startConversationPreview(0);
const browser = await chromium.launch({
  channel: process.env.AIO_UI_BROWSER_CHANNEL,
});
const errors = [];
const results = [];
try {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    colorScheme: "light",
    permissions: ["clipboard-read", "clipboard-write"],
  });
  const page = await context.newPage();
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (item) => {
    if (item.type() === "error") errors.push(item.text());
  });
  await page.goto(origin);
  const frame = page.frameLocator("iframe");
  await frame.getByRole("heading", { name: "今天想完成什么？" }).waitFor();
  const input = frame.getByRole("textbox", { name: "发送消息", exact: true });
  async function assertCall(predicate) {
    for (let attempt = 0; !calls.some(predicate) && attempt < 100; attempt++) {
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    assert(
      calls.some(predicate),
      "业务请求未到达宿主桥：" + JSON.stringify(calls),
    );
  }
  async function geometry(label) {
    const measurements = await frame
      .locator(".dx-conversation")
      .evaluate((root) => {
        const rect = (selector) => {
          const r = root.querySelector(selector).getBoundingClientRect();
          return {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
            right: r.right,
            bottom: r.bottom,
          };
        };
        return {
          width: innerWidth,
          height: innerHeight,
          scrollWidth: document.documentElement.scrollWidth,
          root: root.getBoundingClientRect().toJSON(),
          composer: rect(".dx-conversation__composer"),
          messages: rect(".dx-conversation__messages"),
          surface: getComputedStyle(root).backgroundColor,
          sidebarWidth: root
            .querySelector(".dx-conversation__history")
            .getBoundingClientRect().width,
          sidebarSurface: getComputedStyle(
            root.querySelector(".dx-conversation__history"),
          ).backgroundColor,
          composerSurface: getComputedStyle(
            root.querySelector(".dx-conversation__composer"),
          ).backgroundColor,
          device: rect(".dx-conversation__device-control"),
          model: rect(".dx-conversation__model-control"),
          router: rect(".dx-conversation__router"),
        };
      });
    assert(
      measurements.scrollWidth <= measurements.width,
      `${label}: 页面横向溢出`,
    );
    assert(
      measurements.composer.x >= 0 &&
        measurements.composer.right <= measurements.width,
      `${label}: 输入框溢出`,
    );
    assert(
      measurements.composer.bottom <= measurements.height,
      `${label}: 输入框不可见`,
    );
    assert.equal(
      measurements.root.height,
      measurements.height,
      `${label}: 工作区高度`,
    );
    assert(
      measurements.device.right <= measurements.model.x + 1,
      `${label}: 模型与设备控件重叠`,
    );
    assert(
      measurements.messages.bottom <= measurements.router.y + 1,
      `${label}: 消息与路由面板重叠`,
    );
    if (label === "reference-desktop") {
      assert.equal(measurements.sidebarWidth, 345);
      assert.equal(measurements.sidebarSurface, "rgb(253, 253, 253)");
      assert.equal(measurements.composerSurface, "rgb(255, 255, 255)");
      assert.equal(measurements.composer.width, 752);
      assert.equal(measurements.composer.x, 815);
      assert.equal(measurements.composer.bottom, 1175);
    }
    results.push({ label, ...measurements });
  }
  await geometry("desktop-empty");
  await page.screenshot({ path: `${directory}/desktop-empty.png` });
  assert(
    await frame.getByRole("button", { name: "发送", exact: true }).isDisabled(),
  );
  await frame.getByRole("button", { name: "对话模型", exact: true }).click();
  await frame
    .getByRole("option", { name: "gpt-6-mini · 演示服务", exact: true })
    .click();
  await assertCall(
    (c) =>
      c.method === "PUT" &&
      c.path.endsWith("/model") &&
      c.body.model === "gpt-6-mini",
  );
  await frame
    .getByRole("button", { name: "切换模型 gpt-6 · 演示服务", exact: true })
    .click();
  await assertCall(
    (c) =>
      c.method === "PUT" &&
      c.path.endsWith("/model") &&
      c.body.model === "gpt-6",
  );
  await frame
    .getByRole("button", { name: "刷新模型列表", exact: true })
    .click();
  await assertCall(
    () => calls.filter((c) => c.path === "/providers/models").length >= 2,
  );
  assert.equal(
    await frame.locator(".dx-conversation__router input:disabled").count(),
    3,
  );
  assert.equal(
    await frame.locator(".dx-conversation__router select:disabled").count(),
    3,
  );
  await frame.locator(".dx-conversation__router > summary").click();
  assert(!(await frame.locator(".dx-conversation__router-panel").isVisible()));
  await frame.locator(".dx-conversation__router > summary").click();
  await frame.getByRole("button", { name: "执行设备", exact: true }).click();
  await frame
    .getByRole("option", { name: "本机 · macOS · 在线", exact: true })
    .click();
  await assertCall(
    (c) => c.method === "PUT" && c.path.endsWith("/device") && c.body.workerId,
  );
  await frame.getByRole("button", { name: "更多选项", exact: true }).click();
  await frame.getByRole("button", { name: "蜂群任务", exact: true }).click();
  await frame.getByRole("dialog").waitFor();
  await frame
    .getByText("还没有设备任务。在对话中说明项目和目标，让智能体派发。", {
      exact: true,
    })
    .waitFor();
  assert(
    !calls.some((c) => c.path.endsWith("/messages")),
    "更多菜单不得提交输入表单",
  );
  await page.keyboard.press("Escape");
  await frame.getByRole("button", { name: "搜索对话", exact: true }).click();
  await frame
    .getByRole("textbox", { name: "搜索会话", exact: true })
    .fill("不存在的标题");
  await frame.getByText("没有匹配的对话", { exact: true }).waitFor();
  await frame
    .getByRole("textbox", { name: "搜索会话", exact: true })
    .fill("计划");
  await frame
    .getByRole("button", { name: "为项目梳理下一步计划", exact: true })
    .click();
  await frame.getByRole("heading", { name: "1. 明确当前目标" }).waitFor();
  await geometry("desktop-thread");
  await page.screenshot({ path: `${directory}/desktop-thread.png` });
  await frame.getByRole("button", { name: "搜索对话", exact: true }).click();
  await page.setViewportSize({ width: 2009, height: 1184 });
  await frame.getByRole("button", { name: "环境信息", exact: true }).click();
  await frame.getByRole("complementary", { name: "环境信息" }).waitFor();
  await geometry("reference-desktop");
  await page.screenshot({ path: `${directory}/reference-desktop.png` });
  await frame
    .getByRole("button", { name: "关闭环境信息", exact: true })
    .click();
  assert(
    !(await frame.getByRole("complementary", { name: "环境信息" }).count()),
  );
  await page.setViewportSize({ width: 1440, height: 900 });
  await frame.getByRole("button", { name: "搜索对话", exact: true }).click();
  await frame.locator(".dx-conversation__history-group > summary").click();
  assert(
    !(await frame
      .getByRole("button", { name: "为项目梳理下一步计划", exact: true })
      .isVisible()),
  );
  await frame.locator(".dx-conversation__history-group > summary").click();
  await frame.getByRole("button", { name: "复制回复", exact: true }).click();
  await frame
    .getByRole("button", { name: "已复制回复", exact: true })
    .waitFor();
  assert(
    (await page.evaluate(() => navigator.clipboard.readText())).includes(
      "明确当前目标",
    ),
  );
  await frame
    .getByRole("button", { name: "收起会话列表", exact: true })
    .click();
  assert(
    !(await frame.getByRole("complementary", { name: "会话列表" }).isVisible()),
  );
  await frame
    .getByRole("button", { name: "切换会话列表", exact: true })
    .click();
  assert(
    await frame.getByRole("complementary", { name: "会话列表" }).isVisible(),
  );
  await input.fill("测试中文输入法");
  const before = calls.filter((c) => c.path.endsWith("/messages")).length;
  await input.dispatchEvent("compositionstart", { data: "测试" });
  await input.dispatchEvent("keydown", {
    key: "Enter",
    code: "Enter",
    isComposing: true,
  });
  assert.equal(
    calls.filter((c) => c.path.endsWith("/messages")).length,
    before,
  );
  await input.dispatchEvent("compositionend", { data: "测试" });
  await input.press("Shift+Enter");
  assert.equal(
    calls.filter((c) => c.path.endsWith("/messages")).length,
    before,
  );
  await input.press("Enter");
  await frame.getByRole("button", { name: "停止", exact: true }).waitFor();
  await frame.getByRole("button", { name: "停止", exact: true }).click();
  await frame.getByText("已停止", { exact: true }).waitFor();
  assert(calls.some((c) => c.path.endsWith("/cancel")));
  await input.fill("继续对话");
  await input.press("Enter");
  await frame
    .getByText(
      "已收到。这是本地界面验收的回复，用于检查消息流、滚动和输入框状态。",
      { exact: true },
    )
    .waitFor();
  await page.emulateMedia({ colorScheme: "dark" });
  await frame.getByRole("button", { name: "更多选项", exact: true }).click();
  await frame
    .getByRole("button", { name: "模型与工具设置", exact: true })
    .click();
  const dialog = frame.getByRole("dialog");
  await dialog.waitFor();
  assert.equal(
    await dialog.evaluate((el) => getComputedStyle(el).backgroundColor),
    "rgb(33, 33, 33)",
  );
  await page.screenshot({ path: `${directory}/desktop-settings-dark.png` });
  await page.keyboard.press("Escape");
  await geometry("desktop-dark");
  await page.screenshot({ path: `${directory}/desktop-dark.png` });
  await frame.getByRole("button", { name: "对话模型", exact: true }).click();
  await page.screenshot({ path: `${directory}/desktop-model-menu.png` });
  await page.keyboard.press("Escape");
  await page.setViewportSize({ width: 390, height: 844 });
  await geometry("mobile-dark");
  await page.screenshot({ path: `${directory}/mobile-dark.png` });
  await frame
    .getByRole("button", { name: "切换会话列表", exact: true })
    .click();
  await frame
    .getByRole("complementary", { name: "会话列表" })
    .waitFor({ state: "visible" });
  await page.screenshot({ path: `${directory}/mobile-sidebar.png` });
  await frame
    .getByRole("button", { name: "关闭会话列表", exact: true })
    .click();
  assert(
    !(await frame.getByRole("complementary", { name: "会话列表" }).isVisible()),
  );
  await page.emulateMedia({ colorScheme: "light" });
  await geometry("mobile-light");
  await page.screenshot({ path: `${directory}/mobile-light.png` });
  for (const name of ["对话模型", "执行设备"]) {
    await frame.getByRole("button", { name, exact: true }).click();
    const menu = await frame.getByRole("listbox").boundingBox();
    assert(
      menu.x >= 0 && menu.x + menu.width <= 390,
      `${name}: 手机下拉菜单越界`,
    );
    await page.screenshot({
      path: `${directory}/mobile-${name === "对话模型" ? "model" : "device"}-menu.png`,
    });
    await page.keyboard.press("Escape");
  }
  await frame
    .getByRole("button", { name: "新对话", exact: true })
    .last()
    .click();
  await frame
    .getByRole("textbox", { name: "对话标题", exact: true })
    .fill("验收新对话");
  await frame.getByRole("button", { name: "保存", exact: true }).click();
  await frame.getByRole("heading", { name: "今天想完成什么？" }).waitFor();
  assert(
    calls.some(
      (c) =>
        c.path === "/conversations" &&
        c.method === "POST" &&
        c.body.title === "验收新对话",
    ),
  );
  await geometry("mobile-empty");
  await page.screenshot({ path: `${directory}/mobile-empty.png` });
  await page.setViewportSize({ width: 390, height: 568 });
  await geometry("mobile-compact-empty");
  await page.screenshot({ path: `${directory}/mobile-compact-empty.png` });
  await page.setViewportSize({ width: 390, height: 844 });
  await frame
    .getByRole("button", { name: "切换会话列表", exact: true })
    .click();
  await frame.getByRole("textbox", { name: "搜索会话", exact: true }).fill("");
  await frame
    .getByRole("button", { name: "删除会话 验收新对话", exact: true })
    .click();
  await frame.getByRole("dialog").waitFor();
  assert(!calls.some((c) => c.method === "DELETE"), "打开确认框不得删除数据");
  await page.screenshot({ path: `${directory}/mobile-delete-dialog.png` });
  await page.keyboard.press("Escape");
  assert.deepEqual(errors, [], "浏览器错误");
  await writeFile(
    `${directory}/measurements.json`,
    JSON.stringify(
      { results, errors, actions: calls.map((c) => `${c.method} ${c.path}`) },
      null,
      2,
    ),
  );
  console.log(
    "PASS: reference geometry, desktop/mobile, light/dark, shortcuts, unavailable router controls, environment panel, grouped history, model/device persistence, mobile menus, copy, IME, send/stop, new conversation and delete confirmation",
  );
} finally {
  if (errors.length) console.error(errors);
  await browser.close();
  await new Promise((resolve) => server.close(resolve));
}
