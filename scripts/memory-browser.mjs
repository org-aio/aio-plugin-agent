import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { chromium } from "playwright";
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function eventually(read, predicate) {
  for (let i = 0; i < 150; i++) {
    const value = await read();
    if (predicate(value)) return value;
    await pause(200);
  }
  throw new Error("Browser workflow timed out");
}

export async function verifyBrowser({
  directory,
  backendPort,
  space,
  provider: templateProvider,
  agent,
  canary,
}) {
  const port = Number(process.env.AIO_MEMORY_BROWSER_PORT || 4348);
  const origin = `http://127.0.0.1:${port}`;
  const preview = spawn(process.execPath, ["scripts/preview.mjs"], {
    env: {
      ...process.env,
      AIO_AGENT_DEV_CONFIG: `${directory}/runtime.json`,
      AIO_AGENT_EXTERNAL_BACKEND: "1",
      PORT: String(port),
      AIO_PLUGIN_PORT: String(backendPort),
    },
    stdio: ["ignore", "ignore", "pipe"],
  });
  let previewLog = "";
  preview.stderr.on("data", (bytes) => (previewLog += bytes));
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  await mkdir("test-results", { recursive: true });
  const reports = [];
  const memory = async (method, path, body = null) =>
    agent("POST", "/memory", { method, path, body });
  try {
    for (let i = 0; ; i++) {
      assert(
        preview.exitCode === null && i < 100,
        `Preview failed: ${previewLog}`,
      );
      try {
        if ((await fetch(origin)).ok) break;
      } catch {}
      await pause(100);
    }
    for (const [name, viewport] of [
      ["desktop", { width: 1440, height: 900 }],
      ["mobile", { width: 390, height: 844 }],
    ]) {
      const context = await browser.newContext({
        viewport,
        permissions: ["clipboard-read", "clipboard-write"],
      });
      const page = await context.newPage();
      const frame = page.frameLocator("iframe");
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      page.on("console", (m) => {
        if (m.type() === "error") errors.push(m.text());
      });
      page.on("requestfailed", (r) =>
        errors.push(r.url() + ": " + r.failure()?.errorText),
      );
      const click = async (label) =>
        frame.getByRole("button", { name: label, exact: true }).click();
      const dialog = () => frame.getByRole("dialog").last();
      try {
        const provider = await agent("POST", "/providers", {
          label: `browser-model-${name}`, endpoint: templateProvider.endpoint,
          model: "memory-test", secret: "provider-test-key",
        });
        const conversation = await agent("POST", "/conversations", {
          title: `Dioxus ${name}`,
          providerId: provider.id,
          spaceId: space.id,
        });
        const path = `/conversations/${conversation.id}`;
        const note = await memory("POST", `/nodes?spaceId=${space.id}`, {
          title: `周三例会 ${name}`,
          kind: "NOTE",
          content: "例会每周三举行。",
        });
        await page.goto(origin);
        await frame
          .getByRole("textbox", { name: "发送消息", exact: true })
          .waitFor({ timeout: 60000 });
        await click("设置");
        await click("添加服务");
        assert.equal(
          await dialog().getByLabel("名称", { exact: true }).count(),
          0,
        );
        await dialog()
          .getByRole("textbox", { name: "服务地址", exact: true })
          .fill(provider.endpoint);
        await dialog()
          .getByLabel("API Key", { exact: true })
          .fill("provider-test-key");
        await click("读取模型");
        await click("选择模型");
        await frame
          .getByRole("option", { name: "memory-test", exact: true })
          .click();
        await dialog()
          .getByRole("button", { name: "保存", exact: true })
          .click();
        await frame
          .getByRole("heading", { name: "智能体设置", exact: true })
          .waitFor();
        await page.waitForTimeout(250);
        await page.screenshot({
          path: `test-results/dioxus-${name}-models.png`,
        });
        await click("关闭");
        const settings = await agent("GET", "/settings");
        const configured = settings.providers.find(
          (p) =>
            p.id !== provider.id &&
            p.endpoint === provider.endpoint &&
            p.hasSecret &&
            p.model === "memory-test",
        );
        assert(configured);
        await click("对话模型");
        await frame
          .getByRole("option", {
            name: `slow · ${provider.label}`,
            exact: true,
          })
          .click();
        await eventually(
          () => agent("GET", path),
          (t) => t.conversation.model === "slow",
        );
        await click("对话模型");
        await frame
          .getByRole("option", {
            name: `memory-test · ${provider.label}`,
            exact: true,
          })
          .click();
        await eventually(
          () => agent("GET", path),
          (t) => t.conversation.model === "memory-test",
        );
        const input = frame.getByRole("textbox", {
          name: "发送消息",
          exact: true,
        });
        await input.fill(`解释行内引用验收 ${note.id}，${note.title}`);
        await click("发送");
        await eventually(
          () => agent("GET", path),
          (t) => t.messages.at(-1)?.status === "complete",
        );
        const reference = frame.getByRole("link", {
          name: note.title,
          exact: true,
        });
        await reference.waitFor({ timeout: 30000 });
        assert.equal(
          await frame
            .getByRole("link", { name: "伪造引用", exact: true })
            .count(),
          0,
        );
        await reference.click();
        await frame
          .getByRole("heading", { name: note.title, exact: true })
          .waitFor();
        await click("关闭");
        const before = (await agent("GET", path)).messages.length;
        await click("对话模型");
        await frame
          .getByRole("option", {
            name: `slow · ${provider.label}`,
            exact: true,
          })
          .click();
        await eventually(
          () => agent("GET", path),
          (t) => t.conversation.model === "slow",
        );
        assert.equal((await agent("GET", path)).messages.length, before);
        await input.fill("停止生成验收");
        await click("发送");
        await frame
          .getByRole("button", { name: "停止", exact: true })
          .waitFor({ timeout: 30000 });
        await click("停止");
        await eventually(
          () => agent("GET", path),
          (t) => t.messages.at(-1)?.status === "cancelled",
        );
        await click("对话模型");
        await frame
          .getByRole("option", {
            name: `memory-test · ${provider.label}`,
            exact: true,
          })
          .click();
        await input.fill(
          JSON.stringify({
            username: "alice",
            password: canary,
            note: `浏览器资料 ${name}`,
          }),
        );
        await click("发送");
        const thread = await eventually(
          () => agent("GET", path),
          (t) => t.messages.at(-1)?.memoryStatus === "complete",
        );
        assert(!JSON.stringify(thread).includes(canary));
        await frame
          .locator('article[data-role="user"]')
          .filter({ hasText: `浏览器资料 ${name}` })
          .getByRole("button", { name: "来源资料", exact: true })
          .click();
        await frame
          .getByRole("button", { name: "查看秘密", exact: true })
          .waitFor();
        assert(!(await frame.locator("body").innerText()).includes(canary));
        await click("查看秘密");
        await frame.getByText(canary, { exact: true }).waitFor();
        await click("隐藏秘密");
        await page.screenshot({
          path: `test-results/dioxus-${name}-source.png`,
        });
        await click("关闭");
        await click("知识图谱");
        await frame
          .getByRole("img", { name: "记忆关系图", exact: true })
          .waitFor();
        await page.screenshot({
          path: `test-results/dioxus-${name}-graph.png`,
        });
        await click("收起图谱");
        await frame.getByRole("img", { name: "记忆关系图", exact: true }).waitFor({ state: "hidden" });
        await page.waitForTimeout(250);
        await page.screenshot({ path: `test-results/dioxus-${name}-chat.png` });
        assert.equal(
          await frame
            .locator("body")
            .evaluate((e) => e.scrollWidth <= innerWidth + 1),
          true,
          "Horizontal overflow",
        );
        // 独立设置页通过相同沙箱桥挂载，保存后不重复弹出设置容器。
        await page.goto(`${origin}/?page=settings`);
        await frame
          .getByRole("heading", { name: "模型服务", exact: true })
          .waitFor({ timeout: 60000 });
        assert.equal(
          await frame
            .getByRole("textbox", { name: "发送消息", exact: true })
            .count(),
          0,
        );
        await click("配置网页搜索");
        await dialog()
          .getByLabel("Tavily API Key", { exact: true })
          .fill("synthetic-search-key");
        await dialog().getByText("启用网页搜索", { exact: true }).click();
        await dialog()
          .getByRole("button", { name: "保存", exact: true })
          .click();
        await eventually(
          () => agent("GET", "/settings"),
          (s) => s.webSearch.enabled && s.webSearch.hasSecret,
        );
        assert(
          !(await frame.locator("body").innerText()).includes(
            "synthetic-search-key",
          ),
        );
        await page.screenshot({
          path: `test-results/dioxus-${name}-settings.png`,
        });
        await click("配置网页搜索");
        await dialog().getByText("清除已保存的密钥", { exact: true }).click();
        await dialog()
          .getByRole("button", { name: "保存", exact: true })
          .click();
        await eventually(
          () => agent("GET", "/settings"),
          (s) => !s.webSearch.enabled && !s.webSearch.hasSecret,
        );
        assert.deepEqual(errors, []);
        reports.push({
          name,
          manualUrl: true,
          noName: true,
          discoveredModels: true,
          conversationModel: true,
          historyRetained: true,
          trustedReference: true,
          secrets: true,
          graph: true,
          settingsPage: true,
          searchSettings: true,
          errors,
        });
      } catch (error) {
        console.error(JSON.stringify(errors));
        console.error(await frame.locator("head").innerHTML());
        await page.screenshot({
          path: `test-results/dioxus-${name}-failure.png`,
        });
        await writeFile(
          `test-results/dioxus-${name}-failure.txt`,
          (await frame.locator("body").innerText()).replaceAll(
            canary,
            "[protected]",
          ),
        );
        throw error;
      } finally {
        await context.close();
      }
    }
    await writeFile(
      "test-results/dioxus-browser-report.json",
      JSON.stringify(reports, null, 2),
    );
    console.log("Dioxus desktop/mobile browser checks passed");
  } finally {
    await browser.close();
    if (preview.exitCode === null) {
      await new Promise((resolve) => {
        preview.once("exit", resolve);
        preview.kill("SIGTERM");
      });
    }
  }
}
