// 使用触屏上下文验证窄屏菜单、历史切换和回车行为，不调用真实模型。
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { startConversationPreview } from "./preview-conversation-ui.mjs";

const directory = "test-results/conversation-mobile";
await mkdir(directory, { recursive: true });
const { server, origin, calls } = await startConversationPreview(0);
const browser = await chromium.launch({
  executablePath: process.env.AIO_UI_BROWSER_EXECUTABLE,
  channel: process.env.AIO_UI_BROWSER_CHANNEL,
});
const errors = [];
const results = [];
try {
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
    hasTouch: true,
    colorScheme: "light",
  });
  const page = await context.newPage();
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (item) => {
    if (item.type() === "error") errors.push(item.text());
  });
  await page.goto(origin);
  const frame = page.frameLocator("iframe");
  const button = (name) => frame.getByRole("button", { name, exact: true });
  await button("对话模型").waitFor();
  await button("切换会话列表").click();
  await button("为项目梳理下一步计划").click();
  await frame.getByRole("heading", { name: "1. 明确当前目标" }).waitFor();
  assert(
    !(await frame.getByRole("complementary", { name: "会话列表" }).isVisible()),
  );
  const input = frame.getByRole("textbox", { name: "发送消息", exact: true });
  const writes = () => calls.filter((c) => c.path.endsWith("/messages")).length;
  const before = writes();
  await input.fill("手机第一行");
  await input.press("Enter");
  await input.press("a");
  assert.equal(await input.inputValue(), "手机第一行\na");
  assert.equal(writes(), before, "触屏回车不得发送消息");
  await page.screenshot({ path: `${directory}/touch-newline.png` });
  await button("发送").click();
  await button("停止").waitFor();
  await button("停止").click();
  await frame.getByText("已停止", { exact: true }).waitFor();
  assert.equal(writes(), before + 1);
  await button("执行设备").click();
  await frame.getByRole("option", { name: "本机 · 在线", exact: true }).click();
  await button("本地项目").click();
  await frame
    .getByRole("option", { name: "demo-project", exact: true })
    .click();
  for (const [width, height, theme] of [
    [320, 568, "light"],
    [390, 844, "light"],
    [430, 932, "dark"],
    [390, 420, "dark"],
  ]) {
    await page.setViewportSize({ width, height });
    await page.emulateMedia({ colorScheme: theme });
    const label = `${width}x${height}-${theme}`;
    const layout = await frame.locator(".dx-conversation").evaluate((root) => {
      const rect = (selector) =>
        root.querySelector(selector).getBoundingClientRect().toJSON();
      return {
        width: innerWidth,
        height: innerHeight,
        scrollWidth: document.documentElement.scrollWidth,
        messages: rect("#messages"),
        composer: rect("#composer"),
        send: rect("#send"),
        font: getComputedStyle(root.querySelector("#draft")).fontSize,
      };
    });
    assert(layout.scrollWidth <= width, `${label}: 页面横向溢出`);
    assert(layout.composer.bottom <= height, `${label}: 输入区超出屏幕`);
    assert(layout.messages.height >= 120, `${label}: 消息区被控件挤没`);
    assert(
      layout.send.width >= 44 && layout.send.height >= 44,
      `${label}: 发送触控区域过小`,
    );
    assert(parseFloat(layout.font) >= 16, `${label}: 输入框字体触发手机缩放`);
    // OOPIF 的合成画面可能晚于布局尺寸更新，等待绘制后再保存视觉证据。
    await page.waitForTimeout(150);
    await page.screenshot({ path: `${directory}/${label}.png` });
    for (const name of ["执行设备", "本地项目", "对话模型"]) {
      await button(name).click();
      const menu = frame.getByRole("listbox", { name, exact: true });
      const box = await menu.boundingBox();
      assert(
        box.x >= 0 && box.x + box.width <= width,
        `${label}: ${name}横向越界`,
      );
      assert(
        box.y >= 0 && box.y + box.height <= height,
        `${label}: ${name}纵向越界`,
      );
      await page.screenshot({ path: `${directory}/${label}-${name}.png` });
      await button(name).click();
    }
    await button("更多选项").click();
    await button("刷新设备").waitFor();
    await page.screenshot({ path: `${directory}/${label}-more.png` });
    await button("刷新设备").click();
    assert(!(await frame.locator("#conversation-actions").isVisible()));
    results.push({ label, layout });
  }
  assert.deepEqual(errors, []);
  await writeFile(
    `${directory}/results.json`,
    JSON.stringify({ results, errors }, null, 2),
  );
  console.log(
    "PASS: touch newline/send/stop, history dismissal, 320/390/430px, short viewport, light/dark, model/device/project menus",
  );
} finally {
  await browser.close();
  await new Promise((resolve) => server.close(resolve));
}
