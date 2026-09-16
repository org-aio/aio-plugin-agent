import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { chromium } from "playwright";
import { writeFile } from "node:fs/promises";
const port = 14192;
const preview = spawn(process.execPath, ["scripts/preview.mjs"], {
  env: { ...process.env, PORT: String(port), AIO_AGENT_EXTERNAL_BACKEND: "1" },
  stdio: ["ignore", "pipe", "pipe"],
});
let logs = "";
preview.stdout.on("data", (b) => (logs += b));
preview.stderr.on("data", (b) => (logs += b));
const browser = await chromium.launch({ channel: "chrome", headless: true });
const errors = [];
const report = [];
let activePage;
const calls = [];
try {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(`http://127.0.0.1:${port}`)).ok) break;
    } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  for (const mobile of [false, true]) {
    const context = await browser.newContext({
      viewport: mobile
        ? { width: 390, height: 844 }
        : { width: 1440, height: 900 },
    });
    const page = await context.newPage();
    activePage = page;
    page.on("response", async (response) => {
      if (response.url().endsWith("/invoke")) {
        const req = JSON.parse(response.request().postData());
        const envelope = await response.json().catch(() => ({}));
        if (
          req.path.includes("/input") ||
          req.path === "/devices" ||
          envelope.status >= 400
        ) {
          calls.push({
            path: req.path,
            status: envelope.status,
            body: JSON.parse(
              Buffer.from(envelope.body || []).toString() || "null",
            ),
          });
        }
      }
    });
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(`http://127.0.0.1:${port}`);
    const frame = page.frameLocator("#plugin");
    await frame.getByRole("heading", { name: "继续任务前，请补充" }).waitFor();
    assert.equal(
      await frame.getByRole("button", { name: "提交并继续" }).isDisabled(),
      true,
    );
    await page.reload();
    await frame.getByRole("heading", { name: "继续任务前，请补充" }).waitFor();
    await frame.getByRole("button", { name: /在哪台设备执行/ }).click();
    await frame
      .getByRole("option", { name: "Mac mini · darwin", exact: true })
      .click();
    await frame
      .getByRole("button", { name: /在哪台设备执行/ })
      .filter({ hasText: "Mac mini" })
      .and(frame.locator('[aria-expanded="false"]'))
      .waitFor();
    assert.equal(
      await frame.getByRole("button", { name: "提交并继续" }).isEnabled(),
      true,
    );
    await page.screenshot({
      path: `/tmp/aio-input-${mobile ? "mobile" : "desktop"}.png`,
    });
    const metrics = await page
      .frames()
      .find((f) => f.url().includes("/assets/"))
      .evaluate(() => ({
        width: innerWidth,
        scroll: document.documentElement.scrollWidth,
      }));
    assert(metrics.scroll <= metrics.width + 1, "页面横向溢出");
    if (mobile) {
      await frame.getByRole("button", { name: "提交并继续" }).click();
      await frame
        .getByText(/已在.*打开 QQ，客户端已确认进程 PID 123/)
        .waitFor();
      await frame
        .getByRole("button", { name: "执行设备", exact: true })
        .filter({ hasText: "Mac mini" })
        .waitFor();
    } else {
      await frame.getByRole("button", { name: "稍后回答" }).click();
      await frame.getByRole("button", { name: "回答问题" }).click();
      await frame
        .getByRole("heading", { name: "继续任务前，请补充" })
        .waitFor();
    }
    report.push({ mobile, refreshRetainsQuestion: true, noOverflow: true });
    await context.close();
  }
  assert.deepEqual(errors, []);
  await writeFile(
    "/tmp/aio-input-browser-result.json",
    JSON.stringify({ report, errors }, null, 2),
  );
} catch (error) {
  await activePage
    ?.screenshot({ path: "/tmp/aio-input-failure.png" })
    .catch(() => {});
  console.error(JSON.stringify(calls));
  throw error;
} finally {
  await browser.close();
  preview.kill("SIGTERM");
}
