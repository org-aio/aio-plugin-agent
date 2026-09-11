import assert from "node:assert/strict";
import { createServer } from "node:http";
import { spawn, execFileSync } from "node:child_process";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { chromium } from "playwright";
import { PNG } from "pngjs";

const directory = "build/browser-test";
execFileSync(process.execPath, ["scripts/setup-dev.mjs"], {
  env: { ...process.env, AIO_AGENT_DEV_DIRECTORY: directory },
  stdio: "inherit",
});
const config = JSON.parse(await readFile(`${directory}/runtime.json`, "utf8"));
const upstream = createServer(async (req, res) => {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const input = JSON.parse(Buffer.concat(chunks).toString());
  res.writeHead(200, { "content-type": "text/event-stream" });
  let index = 0;
  const timer = setInterval(
    () => {
      if (index < 10) {
        res.write(
          `data: ${JSON.stringify({ choices: [{ delta: { content: `协议测试片段 ${++index}。` } }] })}\n\n`,
        );
      } else {
        res.end("data: [DONE]\n\n");
        clearInterval(timer);
      }
    },
    input.messages.at(-1).content.includes("停止") ? 600 : 160,
  );
  res.on("close", () => clearInterval(timer));
});
await new Promise((resolve) => upstream.listen(0, "127.0.0.1", resolve));
config.allowedEndpoints = [`http://127.0.0.1:${upstream.address().port}/v1`];
config.allowLoopback = true;
await writeFile(`${directory}/runtime.json`, JSON.stringify(config), {
  mode: 0o600,
});
const url = "http://127.0.0.1:4194/";
const server = spawn(process.execPath, ["scripts/preview.mjs"], {
  env: {
    ...process.env,
    PORT: "4194",
    AIO_PLUGIN_PORT: "4195",
    AIO_AGENT_DEV_CONFIG: `${directory}/runtime.json`,
  },
  stdio: ["ignore", "pipe", "pipe"],
});
let output = "";
server.stdout.on("data", (d) => (output += d));
server.stderr.on("data", (d) => (output += d));
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
for (let i = 0; ; i++) {
  if (server.exitCode !== null || i > 100) throw new Error(output);
  try {
    if ((await fetch(url)).ok) break;
  } catch {}
  await pause(100);
}
await mkdir("test-results", { recursive: true });
const browser = await chromium.launch({ channel: "chrome", headless: true });
const reports = [];
async function click(locator) {
  await locator.waitFor({ timeout: 20000 });
  await pause(150);
  await locator.click({ force: true });
  await locator.page().mouse.move(0, 0);
  await pause(150);
}
async function type(page, locator, text) {
  await click(locator);
  await page.keyboard.press(
    process.platform === "darwin" ? "Meta+A" : "Control+A",
  );
  await page.keyboard.insertText(text);
}
function response(page, method, path) {
  const result = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === "/invoke" &&
      r.request().postDataJSON()?.method === method &&
      r.request().postDataJSON()?.path === path,
    { timeout: 20000 },
  );
  result.catch(() => {});
  return result;
}
async function body(response) {
  const envelope = await response.json();
  assert.equal(envelope.status, 200, Buffer.from(envelope.body).toString());
  return JSON.parse(Buffer.from(envelope.body).toString());
}
async function reload(page) {
  const settings = response(page, "GET", "/settings");
  await page.goto(url);
  await body(await settings);
  await pause(600);
}
async function sdk(frame, method, path, value) {
  return frame
    .locator("body")
    .evaluate((_, r) => window.aioPlugin.json(r.method, r.path, r.value), {
      method,
      path,
      value,
    });
}
try {
  for (const [name, viewport] of [
    ["desktop", { width: 1440, height: 960 }],
    ["mobile", { width: 390, height: 844 }],
  ]) {
    const context = await browser.newContext({ viewport });
    const page = await context.newPage();
    const frame = page.frameLocator("iframe");
    const errors = [],
      requests = [],
      external = [];
    let provider, conversation;
    page.on("pageerror", (e) => errors.push(e.message));
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(m.text());
    });
    page.on("request", (r) => {
      if (new URL(r.url()).pathname === "/invoke")
        requests.push(r.request?.postDataJSON?.() || r.postDataJSON());
      if (!r.url().startsWith(url.slice(0, -1))) external.push(r.url());
    });
    try {
      await reload(page);
      await frame.locator("canvas").first().waitFor();
      await click(frame.getByRole("button", { name: "模型设置", exact: true }));
      await click(frame.getByRole("button", { name: "添加模型", exact: true }));
      let fields = frame.getByRole("textbox").filter({ visible: true });
      await type(page, fields.nth(0), `测试模型 ${name}`);
      await type(page, fields.nth(1), "test-model");
      await type(page, fields.nth(2), "test-secret");
      const saving = response(page, "POST", "/providers");
      await click(frame.getByRole("button", { name: "保存", exact: true }));
      provider = await body(await saving);
      assert(provider.hasSecret);
      assert(!JSON.stringify(provider).includes("test-secret"));
      await reload(page);
      await click(
        frame.getByRole("button", { name: "新建会话", exact: true }).first(),
      );
      fields = frame.getByRole("textbox").filter({ visible: true });
      const title = `浏览器验收 ${name}`;
      await type(page, fields.nth(0), title);
      const creating = response(page, "POST", "/conversations");
      await click(frame.getByRole("button", { name: "创建", exact: true }));
      conversation = await body(await creating);
      await reload(page);
      const select = async () => {
        if (name === "mobile")
          await click(
            frame.getByRole("button", { name: "会话列表", exact: true }),
          );
        const pending = response(
          page,
          "GET",
          `/conversations/${conversation.id}`,
        );
        await click(
          frame
            .getByText(title, { exact: false })
            .filter({ visible: true })
            .first(),
        );
        await body(await pending);
        await pause(300);
      };
      await select();
      fields = frame.getByRole("textbox").filter({ visible: true });
      const draft = fields.last();
      const before = requests.length;
      await context.setOffline(true);
      await type(page, draft, "测试输入留在 Compose 本地");
      await pause(450);
      assert.equal(requests.length, before);
      await context.setOffline(false);
      const sending = response(
        page,
        "POST",
        `/conversations/${conversation.id}/messages`,
      );
      await click(frame.getByRole("button", { name: "发送", exact: true }));
      await body(await sending);
      for (let i = 0; i < 80; i++) {
        const thread = await sdk(
          frame,
          "GET",
          `/conversations/${conversation.id}`,
        );
        if (thread.messages.at(-1)?.status === "complete") break;
        assert(i < 79);
        await pause(100);
      }
      await pause(500);
      await page.screenshot({ path: `test-results/agent-${name}-chat.png` });
      const png = PNG.sync.read(await page.screenshot());
      const colors = new Set();
      for (let i = 0; i < png.data.length; i += 4)
        colors.add(png.data.readUInt32BE(i));
      assert(colors.size > 100);
      await reload(page);
      await select();
      const persisted = await sdk(
        frame,
        "GET",
        `/conversations/${conversation.id}`,
      );
      assert(persisted.messages.at(-1).content.includes("协议测试片段 10"));
      await type(
        page,
        frame.getByRole("textbox").filter({ visible: true }).last(),
        "停止生成验收",
      );
      const stopBounds = await frame
        .getByRole("button", { name: "发送", exact: true })
        .boundingBox();
      assert(stopBounds);
      const again = response(
        page,
        "POST",
        `/conversations/${conversation.id}/messages`,
      );
      await click(frame.getByRole("button", { name: "发送", exact: true }));
      await body(await again);
      const cancelling = response(
        page,
        "POST",
        `/conversations/${conversation.id}/cancel`,
      );
      await page.mouse.move(12, 12);
      await pause(400);
      await page.mouse.click(
        stopBounds.x + stopBounds.width / 2,
        stopBounds.y + stopBounds.height / 2,
      );
      await body(await cancelling);
      await pause(600);
      assert.equal(
        (
          await sdk(frame, "GET", `/conversations/${conversation.id}`)
        ).messages.at(-1).status,
        "cancelled",
      );
      await reload(page);
      await select();
      await click(frame.getByRole("button", { name: "删除会话", exact: true }));
      await click(frame.getByRole("button", { name: "取消", exact: true }));
      assert.equal(
        (await sdk(frame, "GET", `/conversations/${conversation.id}`))
          .conversation.id,
        conversation.id,
      );
      await reload(page);
      await select();
      await click(frame.getByRole("button", { name: "删除会话", exact: true }));
      const deletion = response(
        page,
        "DELETE",
        `/conversations/${conversation.id}`,
      );
      await click(frame.getByRole("button", { name: "确认删除", exact: true }));
      assert.equal((await (await deletion).json()).status, 204);
      conversation = null;
      assert(
        await frame
          .locator("body")
          .evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      );
      assert.deepEqual(errors, []);
      assert.deepEqual(external, []);
      const isolation = await frame.locator("body").evaluate(() => {
        let parentBlocked = false,
          cookieBlocked = false;
        try {
          void parent.document.body;
        } catch {
          parentBlocked = true;
        }
        try {
          void document.cookie;
        } catch {
          cookieBlocked = true;
        }
        return { parentBlocked, cookieBlocked };
      });
      assert.deepEqual(isolation, { parentBlocked: true, cookieBlocked: true });
      reports.push({
        name,
        canvasColors: colors.size,
        localTypingRequests: 0,
        send: true,
        cancel: true,
        refreshPersistence: true,
        deleteConfirmation: true,
        consoleErrors: errors.length,
        liveModel: false,
      });
    } catch (error) {
      await page.screenshot({ path: `test-results/agent-${name}-failure.png` });
      await writeFile(
        `test-results/agent-${name}-accessibility.txt`,
        await frame.locator("body").ariaSnapshot(),
      );
      console.error(errors);
      throw error;
    } finally {
      await context.setOffline(false);
      if (conversation) {
        await sdk(
          frame,
          "POST",
          `/conversations/${conversation.id}/cancel`,
        ).catch(() => {});
        await pause(500);
        await sdk(frame, "DELETE", `/conversations/${conversation.id}`).catch(
          () => {},
        );
      }
      if (provider)
        await sdk(frame, "DELETE", `/providers/${provider.id}`).catch(() => {});
      await context.close();
    }
  }
  await writeFile(
    "test-results/browser-report.json",
    JSON.stringify(reports, null, 2),
  );
  console.log(JSON.stringify(reports, null, 2));
} finally {
  await browser.close();
  await new Promise((resolve) => {
    server.once("exit", resolve);
    server.kill("SIGTERM");
  });
  await new Promise((resolve) => upstream.close(resolve));
}
