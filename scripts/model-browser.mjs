import assert from "node:assert/strict";

export async function verifyModelControls({
  page,
  frame,
  click,
  agent,
  conversation,
  provider,
  name,
}) {
  const selection = `${provider.label} · ${provider.model}`;
  const change = async (current, next, id) => {
    await click(frame.getByRole("button", { name: current, exact: true }));
    const response = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === "/invoke" &&
        response.request().postDataJSON()?.path ===
          `/conversations/${conversation.id}/model`,
    );
    response.catch(() => {});
    await click(frame.getByRole("button", { name: next, exact: true }));
    assert.equal((await (await response).json()).status, 200);
    assert.equal(
      (await agent("GET", `/conversations/${conversation.id}`)).conversation
        .providerId,
      id,
    );
    await page.reload();
    await frame.getByRole("button", { name: next, exact: true }).waitFor();
  };
  const before = await agent("GET", `/conversations/${conversation.id}`);
  await change(selection, "跟随空间模型", null);
  await change("跟随空间模型", selection, provider.id);
  assert.deepEqual(
    (await agent("GET", `/conversations/${conversation.id}`)).messages,
    before.messages,
  );
  await click(frame.getByRole("button", { name: "模型设置", exact: true }));
  await click(
    frame.getByRole("button", { name: `编辑 ${provider.label}`, exact: true }),
  );
  const endpoint = frame.getByRole("textbox", {
    name: "服务地址",
    exact: true,
  });
  await click(endpoint);
  await page.keyboard.press("ControlOrMeta+a");
  const listed = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === "/invoke" &&
      response.request().postDataJSON()?.path === "/providers/models" &&
      JSON.parse(Buffer.from(response.request().postDataJSON().body).toString())
        .endpoint === provider.endpoint,
  );
  listed.catch(() => {});
  await page.keyboard.insertText(`${provider.endpoint}/`);
  const catalog = await (await listed).json();
  assert.equal(catalog.status, 200);
  assert.deepEqual(JSON.parse(Buffer.from(catalog.body).toString()), [
    "memory-test",
    "slow",
  ]);
  const save = await frame
    .getByRole("button", { name: "保存", exact: true })
    .boundingBox();
  assert(save);
  await click(frame.getByRole("button", { name: "未选择", exact: true }));
  await click(frame.getByRole("button", { name: "memory-test", exact: true }));
  await page.screenshot({ path: `test-results/models-${name}.png` });
  const saved = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === "/invoke" &&
      response.request().postDataJSON()?.path === `/providers/${provider.id}`,
  );
  saved.catch(() => {});
  // Compose 弹出菜单关闭后残留旧语义树，使用打开菜单前的保存按钮位置。
  await page.mouse.click(save.x + save.width / 2, save.y + save.height / 2);
  assert.equal((await (await saved).json()).status, 200);
  await page.reload();
  await frame.getByRole("button", { name: selection, exact: true }).waitFor();
  await page.screenshot({ path: `test-results/model-switch-${name}.png` });
  return { manualEndpoint: true, modelCatalog: true, switchModel: true };
}
