import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";

export async function verifyGeneration({
  agent,
  space,
  endpoint,
  eventually,
  startAgent,
  stopAgent,
  pool,
  canary,
  blockedRecalls,
  modelRequests,
}) {
  assert.equal((await agent("GET", "/settings")).memoryAvailable, true);
  await agent(
    "POST",
    "/providers",
    {
      label: "unapproved",
      model: "x",
      endpoint: "https://unapproved.example/v1",
    },
    "developer",
    400,
  );
  const create = async (model) => {
    const provider = await agent("POST", "/providers", {
      label: model,
      model,
      endpoint,
      secret: "provider-test-key",
    });
    const conversation = await agent("POST", "/conversations", {
      title: "生成恢复验收",
      providerId: provider.id,
      spaceId: space.id,
    });
    await agent(
      "GET",
      `/conversations/${conversation.id}`,
      undefined,
      "outsider",
      404,
    );
    return { provider, conversation };
  };
  const { provider, conversation } = await create("slow");
  const catalog = (body, user = "developer", status = 200) =>
    agent("POST", "/providers/models", body, user, status);
  const expected = ["memory-test", "slow"];
  assert.deepEqual(
    await catalog({ endpoint, secret: "provider-test-key" }),
    expected,
  );
  assert.deepEqual(
    await catalog({ endpoint: `${endpoint}/`, providerId: provider.id }),
    expected,
  );
  await catalog({ endpoint, providerId: provider.id }, "outsider", 404);
  await catalog(
    { endpoint, providerId: provider.id, secret: "" },
    "developer",
    400,
  );
  await catalog({ endpoint, secret: "redirect" }, "developer", 400);
  await catalog({ endpoint, secret: "invalid-response" }, "developer", 400);
  await catalog(
    { endpoint: "https://unapproved.example/v1", secret: "provider-test-key" },
    "developer",
    400,
  );
  const path = `/conversations/${conversation.id}`;
  const send = () =>
    agent("POST", `${path}/messages`, {
      requestId: randomUUID(),
      content: "只使用净化资料的生成验证",
    });
  const partial = () =>
    eventually(
      () => agent("GET", path),
      (thread) =>
        thread.messages.at(-1)?.status === "generating" &&
        thread.messages.at(-1).content.length > 0,
    );
  await send();
  await partial();
  await agent("PUT", `${path}/model`, { providerId: null }, "developer", 409);
  await agent("POST", `${path}/cancel`);
  await eventually(
    () => agent("GET", path),
    (thread) => thread.messages.at(-1)?.status === "cancelled",
  );
  await send();
  await partial();
  await eventually(
    () =>
      pool.query("SELECT state FROM agent_intake WHERE conversation_id=$1", [
        conversation.id,
      ]),
    (result) => result.rows.every((row) => row.state === "complete"),
  );
  await stopAgent("SIGKILL");
  await startAgent();
  const interrupted = await agent("GET", path);
  assert.equal(interrupted.messages.at(-1).status, "interrupted");
  assert(interrupted.messages.at(-1).content.length > 0);
  await agent(
    "DELETE",
    `/providers/${provider.id}`,
    undefined,
    "developer",
    409,
  );
  for (const model of ["fail", "broken"]) {
    const { conversation } = await create(model);
    await agent("POST", `/conversations/${conversation.id}/messages`, {
      requestId: randomUUID(),
      content: "模型异常时资料仍需保留",
    });
    const failed = await eventually(
      () => agent("GET", `/conversations/${conversation.id}`),
      (thread) => thread.messages.at(-1)?.status === "failed",
    );
    assert(!JSON.stringify(failed).includes(canary));
    assert(failed.messages[0].sourceId);
  }
  const shared = await agent("POST", "/providers", {
    label: "空间问答",
    model: "memory-test",
    endpoint,
    secret: "provider-test-key",
  });
  await agent(
    "PUT",
    `${path}/model`,
    { providerId: shared.id },
    "outsider",
    404,
  );
  const outsider = await agent(
    "POST",
    "/providers",
    {
      label: "独立凭据",
      model: "memory-test",
      endpoint,
    },
    "outsider",
  );
  await agent(
    "PUT",
    `${path}/model`,
    { providerId: outsider.id },
    "developer",
    404,
  );
  await agent(
    "DELETE",
    `/providers/${outsider.id}`,
    undefined,
    "outsider",
    204,
  );
  const beforeSwitch = await agent("GET", path);
  const switched = await agent("PUT", `${path}/model`, {
    providerId: shared.id,
  });
  assert.equal(switched.id, conversation.id);
  assert.equal(switched.providerId, shared.id);
  assert.deepEqual((await agent("GET", path)).messages, beforeSwitch.messages);
  const callsBefore = modelRequests.length;
  await send();
  const continued = await eventually(
    () => agent("GET", path),
    (thread) =>
      thread.messages.length === beforeSwitch.messages.length + 2 &&
      thread.messages.at(-1)?.status === "complete",
  );
  assert.equal(continued.messages.at(-1).content, "已记录。");
  assert(
    modelRequests
      .slice(callsBefore)
      .some(
        (request) =>
          request.model === "memory-test" && request.messages.length > 2,
      ),
  );
  assert.equal(
    (await agent("PUT", `${path}/model`, { providerId: null })).providerId,
    null,
  );
  const pendingSpace = await agent("POST", "/memory", {
    method: "POST",
    path: "/spaces",
    body: { title: "先收件后配置" },
  });
  const pendingConversation = await agent("POST", "/conversations", {
    title: "继续原对话",
    spaceId: pendingSpace.id,
  });
  const pendingPath = `/conversations/${pendingConversation.id}`;
  const question = () =>
    agent("POST", `${pendingPath}/messages`, {
      requestId: randomUUID(),
      content: "项目会议定在周五，请分析安排",
    });
  await question();
  const received = await eventually(
    () => agent("GET", pendingPath),
    (thread) => thread.messages.at(-1)?.status === "complete",
  );
  assert(received.messages.at(-1).content.includes("模型配置完成后"));
  await agent("POST", "/memory", {
    method: "PUT",
    path: `/spaces/${pendingSpace.id}`,
    body: { title: pendingSpace.title, modelBinding: shared.id },
  });
  await question();
  const answered = await eventually(
    () => agent("GET", pendingPath),
    (thread) =>
      thread.messages.length === 4 &&
      thread.messages.at(-1)?.status === "complete",
  );
  assert.equal(answered.messages.at(-1).content, "已记录。");
  await agent("POST", "/memory", {
    method: "POST",
    path: `/spaces/${pendingSpace.id}/members`,
    body: { userId: "editor", role: "EDITOR" },
  });
  const team = await agent(
    "POST",
    "/conversations",
    { title: "共享空间模型", spaceId: pendingSpace.id },
    "editor",
  );
  await agent(
    "POST",
    `/conversations/${team.id}/messages`,
    { requestId: randomUUID(), content: "请分析会议安排" },
    "editor",
  );
  const sharedAnswer = await eventually(
    () => agent("GET", `/conversations/${team.id}`, undefined, "editor"),
    (thread) => thread.messages.at(-1)?.status === "complete",
  );
  assert.equal(sharedAnswer.messages.at(-1).content, "已记录。");
  await agent("POST", "/memory", {
    method: "PUT",
    path: `/spaces/${pendingSpace.id}`,
    body: { title: pendingSpace.title, modelBinding: null },
  });
  await question();
  const unbound = await eventually(
    () => agent("GET", pendingPath),
    (thread) =>
      thread.messages.length === 6 &&
      thread.messages.at(-1)?.status === "complete",
  );
  assert(unbound.messages.at(-1).content.includes("模型配置完成后"));
  await agent("POST", "/memory", {
    method: "PUT",
    path: `/spaces/${pendingSpace.id}`,
    body: { title: pendingSpace.title, modelBinding: shared.id },
  });
  blockedRecalls.set(pendingSpace.id, 0);
  try {
    await question();
    await eventually(
      async () => blockedRecalls.get(pendingSpace.id),
      (count) => count > 0,
    );
    const available = await create("memory-test");
    const availablePath = `/conversations/${available.conversation.id}`;
    await agent("POST", `${availablePath}/messages`, {
      requestId: randomUUID(),
      content: "其他空间的资料继续处理",
    });
    await Promise.race([
      eventually(
        () => agent("GET", availablePath),
        (thread) => thread.messages.at(-1)?.status === "complete",
      ),
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error("单条失败阻塞其他收件")), 8000),
      ),
    ]);
  } finally {
    blockedRecalls.delete(pendingSpace.id);
  }
  const recovered = await eventually(
    () => agent("GET", pendingPath),
    (thread) =>
      thread.messages.length === 8 &&
      thread.messages.at(-1)?.status === "complete",
  );
  assert.equal(recovered.messages.at(-1).content, "已记录。");
}
