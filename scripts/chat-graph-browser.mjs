import assert from "node:assert/strict";
import { PNG } from "pngjs";

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export async function verifyChatGraph({
  page,
  frame,
  click,
  agent,
  conversation,
  name,
}) {
  if (name === "desktop") {
    for (let index = 0; index < 12; index++) {
      await agent("POST", "/memory", {
        method: "POST",
        path: `/nodes?spaceId=${conversation.spaceId}`,
        body: { title: `资料 ${index + 1}`, kind: "NOTE", content: "布局验收" },
      });
    }
  }
  await click(frame.getByRole("textbox", { name: "消息输入", exact: true }));
  await page.keyboard.insertText("查找 测试项目");
  await click(frame.getByRole("button", { name: "发送", exact: true }));
  let answer;
  for (let attempt = 0; ; attempt++) {
    const thread = await agent("GET", `/conversations/${conversation.id}`);
    answer = thread.messages.at(-1);
    if (
      thread.messages.length === 4 &&
      answer?.route === "recall" &&
      answer.status === "complete"
    )
      break;
    assert(attempt < 150, "Local lookup did not finish");
    await pause(200);
  }
  assert.equal(answer.tokens, 0);
  assert(answer.matchedNodeIds.length > 0);
  await pause(2500);
  const graph = frame.getByLabel(/^知识图谱，\d+ 个节点，\d+ 个激活节点$/);
  await graph.waitFor({ timeout: 15000 });
  const label = await graph.getAttribute("aria-label");
  assert(Number(label.match(/，(\d+) 个激活/)[1]) > 0);
  await click(frame.getByRole("button", { name: "暂停图谱布局", exact: true }));
  const bounds = await graph.boundingBox();
  assert(bounds && bounds.width > 200 && bounds.height > 100);
  const pixels = PNG.sync.read(
    await page.screenshot({ path: `test-results/graph-${name}-active.png` }),
  );
  let highlightedPixels = 0;
  let highlightedPoint;
  for (
    let y = Math.max(0, Math.ceil(bounds.y));
    y < Math.min(pixels.height, bounds.y + bounds.height);
    y++
  ) {
    for (
      let x = Math.max(0, Math.ceil(bounds.x));
      x < Math.min(pixels.width, bounds.x + bounds.width);
      x++
    ) {
      const i = (y * pixels.width + x) * 4;
      const [r, g, b] = pixels.data.subarray(i, i + 3);
      if (r > 135 && r < 210 && g > 65 && g < 145 && b < 70 && r > g + 35) {
        highlightedPixels++;
        highlightedPoint ??= { x, y: y + 3 };
      }
    }
  }
  assert(
    highlightedPixels > 100,
    "Activated nodes were not painted in the graph",
  );
  await page.mouse.click(highlightedPoint.x, highlightedPoint.y);
  const open = frame.getByRole("button", { name: "打开记忆条目", exact: true });
  for (let attempt = 0; !(await open.isEnabled()); attempt++) {
    assert(attempt < 30, "Graph node picking did not select an entry");
    await pause(100);
  }
  await click(frame.getByRole("button", { name: "放大图谱", exact: true }));
  await click(frame.getByRole("button", { name: "缩小图谱", exact: true }));
  const toggle = frame.getByRole("button", {
    name: "切换图谱视图",
    exact: true,
  });
  await click(toggle);
  await page.screenshot({ path: `test-results/graph-${name}-list.png` });
  await click(toggle);
  await click(frame.getByRole("button", { name: "收起图谱", exact: true }));
  await click(frame.getByRole("button", { name: "切换知识图谱", exact: true }));
  await pause(1500);
  await page.screenshot({ path: `test-results/graph-${name}-reopened.png` });
  return {
    localLookup: true,
    activated: answer.activatedNodeIds.length,
    highlightedPixels,
    graphControls: true,
    nodePicking: true,
  };
}
