import assert from "node:assert/strict";

export async function verifyMessageBody({
  page,
  frame,
  click,
  agent,
  conversation,
  name,
}) {
  const send = async (text) => {
    await click(frame.getByRole("textbox", { name: "消息输入", exact: true }));
    await page.keyboard.insertText(text);
    await click(frame.getByRole("button", { name: "发送", exact: true }));
    for (let attempt = 0; attempt < 150; attempt++) {
      const thread = await agent("GET", `/conversations/${conversation.id}`);
      if (
        thread.messages.at(-2)?.content === text &&
        thread.messages.at(-1)?.status === "complete"
      ) {
        // Compose beta 的动态列表语义树在恢复会话后才完整，画面仍由真实 Compose 绘制。
        await page.reload();
        await frame
          .getByRole("button", { name: "删除会话", exact: true })
          .waitFor();
        return thread.messages.at(-1);
      }
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
    throw new Error("Message did not complete");
  };
  const greeting = await send("hi");
  assert.equal(greeting.route, "greeting");
  assert.equal(greeting.tokens, 0);
  assert.deepEqual(greeting.citations, []);
  await frame
    .getByText("你好！可以直接发资料让我记住，也可以问我之前记录的内容。", {
      exact: true,
    })
    .waitFor();
  const node = await agent("POST", "/memory", {
    method: "POST",
    path: `/nodes?spaceId=${conversation.spaceId}`,
    body: {
      title: `行内引用验收 ${name}`,
      kind: "NOTE",
      content: "例会是周三",
    },
  });
  const answer = await send(`解释行内引用验收 ${node.id}`);
  assert(answer.citations.some((citation) => citation.id === node.id));
  assert(answer.content.includes(`memory:${node.id}`));
  const unavailable = frame.getByText(/伪造引用（来源不可用）/);
  await unavailable.waitFor();
  assert.equal(await frame.getByText(/\]\(memory:/).count(), 0);
  // 当前 Compose 将行内 Clickable 注解暴露为无名按钮；具名来源按钮仍在正文下方。
  const link = frame.getByRole("button", { name: "", exact: true });
  await link.waitFor();
  assert.equal(
    await link.count(),
    1,
    "Only the authorized inline citation is clickable",
  );
  await page.screenshot({ path: `test-results/message-${name}.png` });
  const opened = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === "/invoke" &&
      response.request().postDataJSON()?.path === "/memory" &&
      JSON.parse(Buffer.from(response.request().postDataJSON().body).toString())
        .path === `/nodes/${node.id}`,
  );
  await click(link);
  assert.equal((await (await opened).json()).status, 200);
  await frame.getByText("例会是周三", { exact: true }).waitFor();
  await page.screenshot({ path: `test-results/message-${name}-opened.png` });
  assert(
    await frame
      .locator("body")
      .evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  );
  return {
    greetingTokens: 0,
    inlineCitation: true,
    invalidCitationDisabled: true,
    openedEntry: true,
  };
}
