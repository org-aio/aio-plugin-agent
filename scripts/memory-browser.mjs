import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { PNG } from "pngjs";

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function openSource(page, frame) {
  const button = frame
    .getByRole("button", { name: "已整理", exact: true })
    .first();
  if (await button.isVisible()) {
    await button.click({ force: true });
    return;
  }
  // Compose beta 的动态列表会丢失语义节点；依据实际画布上的首个来源按钮定位。
  const input = await frame
    .getByRole("textbox", { name: "消息输入", exact: true })
    .boundingBox();
  const header = await frame
    .getByRole("button", { name: "删除会话", exact: true })
    .boundingBox();
  assert(input && header);
  const png = PNG.sync.read(await page.screenshot());
  for (let y = Math.ceil(header.y + header.height + 20); y < input.y; y++) {
    for (
      let x = Math.ceil(input.x);
      x < Math.min(input.x + 180, png.width);
      x++
    ) {
      const offset = (y * png.width + x) * 4;
      const [r, g, b] = png.data.subarray(offset, offset + 3);
      if (r < 60 && g > 80 && g < 165 && b > 40 && b < 145 && g > r + 40) {
        await page.mouse.click(x + 12, y + 6);
        await pause(300);
        return;
      }
    }
  }
  assert.fail("Source control was absent from the rendered canvas");
}

export async function verifyBrowser({
  directory,
  backendPort,
  space,
  provider,
  agent,
  canary,
}) {
  provider = await agent("PUT", `/providers/${provider.id}`, {
    label: "浏览器模型",
    endpoint: provider.endpoint,
    model: provider.model,
  });
  const port = Number(process.env.AIO_MEMORY_BROWSER_PORT || 4298);
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
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  await mkdir("test-results", { recursive: true });
  const reports = [];
  try {
    for (let i = 0; ; i++) {
      assert(preview.exitCode === null && i < 100, "Preview startup failed");
      try {
        if ((await fetch(origin)).ok) break;
      } catch {}
      await pause(100);
    }
    for (const [name, viewport] of [
      ["desktop", { width: 1440, height: 960 }],
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
      page.on("console", (message) => {
        if (message.type() === "error") errors.push(message.text());
      });
      const click = async (locator) => {
        await locator.waitFor({ timeout: 20000 });
        await pause(250);
        await locator.click({ force: true });
        await page.mouse.move(0, 0);
        await pause(200);
      };
      try {
        const conversation = await agent("POST", "/conversations", {
          title: `浏览器记忆 ${name}`,
          providerId: provider.id,
          spaceId: space.id,
        });
        await page.goto(origin);
        await frame.locator("canvas").first().waitFor({ timeout: 60000 });
        await frame
          .getByRole("button", { name: "删除会话", exact: true })
          .waitFor();
        const input = frame.getByRole("textbox", {
          name: "消息输入",
          exact: true,
        });
        await click(input);
        await page.keyboard.insertText(
          JSON.stringify({
            project: "浏览器项目",
            password: canary,
            note: "会议记录已完成",
          }),
        );
        const sent = page.waitForResponse(
          (response) =>
            new URL(response.url()).pathname === "/invoke" &&
            response.request().postDataJSON()?.path ===
              `/conversations/${conversation.id}/messages`,
          { timeout: 15000 },
        );
        sent.catch(() => {});
        await click(frame.getByRole("button", { name: "发送", exact: true }));
        assert.equal((await (await sent).json()).status, 200);
        for (let i = 0; ; i++) {
          const thread = await agent(
            "GET",
            `/conversations/${conversation.id}`,
          );
          if (thread.messages.at(-1)?.memoryStatus === "complete") break;
          assert(i < 150, "Browser message did not finish");
          await pause(200);
        }
        await pause(2200);
        await openSource(page, frame);
        const close = frame
          .getByRole("button", { name: "关闭", exact: true })
          .last();
        const closeBounds = await close.boundingBox();
        const toggle = frame.getByRole("button", {
          name: "查看秘密",
          exact: true,
        });
        const revealed = page.waitForResponse(
          (response) =>
            new URL(response.url()).pathname === "/invoke" &&
            response.request().postDataJSON()?.path === "/memory" &&
            JSON.parse(
              Buffer.from(response.request().postDataJSON().body).toString(),
            ).path.endsWith("/reveal"),
        );
        revealed.catch(() => {});
        await click(toggle);
        const protectedResponse = await (await revealed).json();
        assert.equal(
          JSON.parse(Buffer.from(protectedResponse.body).toString()).value,
          canary,
        );
        await page.screenshot({
          path: `test-results/memory-${name}-revealed.png`,
        });
        await click(
          frame.getByRole("button", { name: "复制秘密", exact: true }),
        );
        assert.equal(
          await page.evaluate(() => navigator.clipboard.readText()),
          canary,
        );
        await click(
          (await frame
            .getByRole("button", { name: "隐藏秘密", exact: true })
            .isVisible())
            ? frame.getByRole("button", { name: "隐藏秘密", exact: true })
            : toggle,
        );
        await page.screenshot({
          path: `test-results/memory-${name}-source.png`,
        });
        if (await close.isVisible()) await click(close);
        else {
          assert(closeBounds, "Source dialog close control was absent");
          await page.mouse.click(
            closeBounds.x + closeBounds.width / 2,
            closeBounds.y + closeBounds.height / 2,
          );
          await page.mouse.move(0, 0);
          await pause(300);
        }
        await page.screenshot({ path: `test-results/memory-${name}-chat.png` });
        const png = PNG.sync.read(await page.screenshot());
        const colors = new Set();
        for (let i = 0; i < png.data.length; i += 4)
          colors.add(png.data.readUInt32BE(i));
        assert(colors.size > 100, "Compose canvas is blank");
        assert(
          await frame
            .locator("body")
            .evaluate(() => document.documentElement.scrollWidth <= innerWidth),
        );
        assert.deepEqual(errors, []);
        // Compose beta 关闭原生 Dialog 后会保留旧语义树；独立图谱流程从恢复的会话开始。
        await page.reload();
        await frame.locator("canvas").first().waitFor({ timeout: 60000 });
        const { verifyChatGraph } = await import("./chat-graph-browser.mjs");
        const graphReport = await verifyChatGraph({
          page,
          frame,
          click,
          agent,
          conversation,
          name,
        });
        await page.reload();
        await frame.locator("canvas").first().waitFor();
        assert.equal(
          (await agent("GET", `/conversations/${conversation.id}`)).messages
            .length,
          4,
        );
        const { verifyModelControls } = await import("./model-browser.mjs");
        const modelReport = await verifyModelControls({
          page,
          frame,
          click,
          agent,
          conversation,
          provider,
          name,
        });
        reports.push({
          name,
          canvasColors: colors.size,
          send: true,
          reveal: true,
          copy: true,
          hide: true,
          reload: true,
          consoleErrors: errors.length,
          ...graphReport,
          ...modelReport,
        });
      } catch (error) {
        await page.screenshot({
          path: `test-results/memory-${name}-failure.png`,
        });
        await writeFile(
          `test-results/memory-${name}-accessibility.txt`,
          await frame.locator("body").ariaSnapshot(),
        );
        throw error;
      } finally {
        await context.close();
      }
    }
    await writeFile(
      "test-results/memory-browser-report.json",
      JSON.stringify(reports, null, 2),
    );
    console.log(JSON.stringify(reports));
  } finally {
    await browser.close();
    if (preview.exitCode === null)
      await new Promise((resolve) => {
        preview.once("exit", resolve);
        preview.kill("SIGTERM");
      });
  }
}
