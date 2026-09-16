import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";

export async function verifyRouting({
  agent,
  memory,
  eventually,
  modelRequests,
  canary,
  provider,
}) {
  const requestsBefore = modelRequests.length;
  const space = await agent("POST", "/memory", {
    method: "POST",
    path: "/spaces",
    body: { title: "本地分流验收", modelBinding: provider.id },
  });
  const inSpace = (path) => `${path}?spaceId=${space.id}`;
  const note = (
    await memory("POST", inSpace("/nodes"), {
      title: "极光项目",
      content: "例会时间：周三 14:00",
      aliases: ["Aurora"],
      kind: "PROJECT",
    })
  ).body;
  const person = (
    await memory("POST", inSpace("/nodes"), {
      title: "负责人林青",
      kind: "PERSON",
      content: "负责验收",
    })
  ).body;
  const edge = await memory("POST", inSpace("/edges"), {
    source: note.id,
    target: person.id,
    relation: "负责人",
    evidence: "项目记录",
  });
  assert.equal(edge.status, 201);
  const conversation = await agent("POST", "/conversations", {
    title: "边聊边查",
    spaceId: space.id,
  });
  const path = `/conversations/${conversation.id}`;
  let count = 0;
  const send = async (content) => {
    const prompt = { requestId: randomUUID(), content };
    await agent("POST", `${path}/messages`, prompt);
    count += 2;
    const thread = await eventually(
      () => agent("GET", path),
      (value) =>
        value.messages.length === count &&
        value.messages.at(-1)?.status === "complete",
    );
    return { thread, prompt, answer: thread.messages.at(-1) };
  };
  const lookup = await send("查找 Aurora");
  assert.equal(lookup.answer.route, "recall");
  assert.equal(lookup.answer.tokens, 0);
  assert(lookup.answer.content.includes("周三 14:00"));
  assert.deepEqual(lookup.answer.matchedNodeIds, [note.id]);
  assert(lookup.answer.activatedNodeIds.includes(person.id));
  const lookupSource = lookup.thread.messages[0].sourceId;
  assert.equal(
    (await memory("GET", `/sources/${lookupSource}`)).body.status,
    "recorded",
  );
  assert.equal(
    (await memory("GET", inSpace("/graph"))).body.nodes.some(
      (n) => n.id === lookupSource,
    ),
    false,
  );
  await agent("POST", `${path}/messages`, lookup.prompt);
  assert.equal((await agent("GET", path)).messages.length, 2);

  for (const content of ["hi", "你好！", "Hi!"]) {
    const greeted = await send(content);
    assert.equal(greeted.answer.route, "greeting");
    assert.equal(greeted.answer.tokens, 0);
    assert(greeted.answer.content.startsWith("你好！"));
    assert.deepEqual(greeted.answer.citations, []);
    assert.deepEqual(greeted.answer.activatedNodeIds, []);
    const source = greeted.thread.messages.at(-2).sourceId;
    assert.equal(
      (await memory("GET", `/sources/${source}`)).body.status,
      "recorded",
    );
    assert(
      !(await memory("GET", inSpace("/graph"))).body.nodes.some(
        (n) => n.id === source,
      ),
    );
    await agent("POST", `${path}/messages`, greeted.prompt);
    assert.equal((await agent("GET", path)).messages.length, count);
  }
  const claim = await memory(
    "POST",
    "/tasks/claim",
    { spaceId: space.id },
    "developer",
    true,
  );
  assert.equal(claim.status, 200);
  assert.equal(claim.body, null);
  assert.equal(
    modelRequests.length,
    requestsBefore,
    "Greetings and lookup must not call the model",
  );

  const empty = await send("查找 完全不存在的条目");
  assert.equal(empty.answer.route, "recall");
  assert.deepEqual(empty.answer.activatedNodeIds, []);
  assert.deepEqual(empty.answer.citations, []);
  assert(empty.answer.content.includes("没有找到"));
  const saved = await send(`项目：星河\npassword: ${canary}`);
  assert.equal(saved.answer.route, "save");
  assert.equal(saved.answer.tokens, 0);
  assert(!JSON.stringify(saved.thread).includes(canary));
  const credential = await send("查找 星河密码");
  assert.equal(credential.answer.route, "recall");
  assert.equal(credential.answer.tokens, 0);
  assert(credential.answer.citations.length > 0);
  assert(!JSON.stringify(credential.thread).includes(canary));
  assert(credential.answer.content.includes("[保密字段]"));
  assert(!credential.answer.content.includes("[[secret:"));

  const activated = await agent("POST", "/memory", {
    method: "POST",
    path: inSpace("/activation"),
    body: { nodeIds: [note.id] },
  });
  assert(activated.nodes.some((n) => n.id === note.id));
  assert(activated.nodes.some((n) => n.id === person.id));
  assert(activated.nodes.every((n) => n.content === ""));
  assert(!JSON.stringify(activated).includes(canary));
  assert.equal(
    (
      await memory(
        "POST",
        inSpace("/activation"),
        { nodeIds: [note.id] },
        "outsider",
      )
    ).status,
    403,
  );
  await memory("POST", `/spaces/${space.id}/members`, {
    userId: "graph-reader",
    role: "READER",
  });
  assert.equal(
    (
      await memory(
        "POST",
        inSpace("/activation"),
        { nodeIds: [note.id] },
        "graph-reader",
      )
    ).status,
    200,
  );
  await memory("DELETE", `/spaces/${space.id}/members/graph-reader`);
  assert.equal(
    (
      await memory(
        "POST",
        inSpace("/activation"),
        { nodeIds: [note.id] },
        "graph-reader",
      )
    ).status,
    403,
  );
  const foreign = (await memory("POST", "/spaces", { title: "隔离空间" })).body;
  assert.equal(
    (
      await memory("POST", `/route?spaceId=${foreign.id}`, {
        sourceId: lookupSource,
      })
    ).status,
    404,
  );
  const cross = (
    await memory("POST", `/activation?spaceId=${foreign.id}`, {
      nodeIds: [note.id],
    })
  ).body;
  assert.equal(
    cross.nodes.some((n) => n.id === note.id),
    false,
  );
  assert(
    !modelRequests.some((request) =>
      JSON.stringify(request).includes("查找 Aurora"),
    ),
  );

  await memory("DELETE", `/nodes/${note.id}`);
  const hidden = await agent("GET", path);
  assert.equal(hidden.messages[1].memoryStatus, "unavailable");
  assert.deepEqual(hidden.messages[1].activatedNodeIds, []);
  const removed = (
    await memory("POST", inSpace("/activation"), { nodeIds: [note.id] })
  ).body;
  assert(!removed.nodes.some((n) => n.id === note.id));
  console.log(
    "Local routing, activation, idempotency, scope and deletion checks passed",
  );
}
