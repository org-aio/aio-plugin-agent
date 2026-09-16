import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";

const title = "运行时工具专用条目";

export function messageText(message) {
  return typeof message.content === "string"
    ? message.content
    : message.content
        .filter((part) => part.type === "text")
        .map((part) => part.text)
        .join("\n");
}

export function runtimeModelResponse(input, response, authorization) {
  if (input.model !== "runtime-tool") return false;
  assert.equal(authorization, "Bearer runtime-provider-key");
  assert.deepEqual(
    input.tools.map((tool) => tool.function.name),
    ["memory_search"],
  );
  const results = input.messages.filter((message) => message.role === "tool");
  if (results.length) assert(results[0].content.includes("没有找到相关资料"), JSON.stringify(results[0]));
  const complete = results.length === 2;
  const delta = complete
    ? { content: "已通过 Rust 工具读取记忆。" }
    : {
        tool_calls: [
          {
            index: 0,
            id: `memory_lookup_${results.length}`,
            type: "function",
            function: {
              name: "memory_search",
              arguments: JSON.stringify({
                query: results.length ? title : "请分析执行结果",
              }),
            },
          },
        ],
      };
  if (complete)
    assert(results[1].content.includes("仅工具检索可得到的验收内容"));
  response.writeHead(200, { "content-type": "text/event-stream" }).end(
    [
      `data: ${JSON.stringify({ choices: [{ index: 0, delta, finish_reason: null }] })}\n\n`,
      `data: ${JSON.stringify({
        choices: [
          {
            index: 0,
            delta: {},
            finish_reason: complete ? "stop" : "tool_calls",
          },
        ],
        usage: { prompt_tokens: 10, completion_tokens: 3, total_tokens: 13 },
      })}\n\n`,
      "data: [DONE]\n\n",
    ].join(""),
  );
  return true;
}

export async function verifyRuntime({
  agent,
  memory,
  eventually,
  space,
  endpoint,
  modelRequests,
}) {
  const note = (
    await memory("POST", `/nodes?spaceId=${space.id}`, {
      title,
      kind: "NOTE",
      content: "仅工具检索可得到的验收内容",
    })
  ).body;
  const provider = await agent("POST", "/providers", {
    label: "Rust 运行时验收",
    model: "runtime-tool",
    endpoint,
    secret: "runtime-provider-key",
  });
  const conversation = await agent("POST", "/conversations", {
    title: "Rust 工具循环",
    providerId: provider.id,
    spaceId: space.id,
  });
  const path = `/conversations/${conversation.id}`;
  await agent("POST", `${path}/messages`, {
    requestId: randomUUID(),
    content: "请分析执行结果",
  });
  const thread = await eventually(
    () => agent("GET", path),
    (value) => value.messages.at(-1)?.status === "complete",
  );
  const reply = thread.messages.at(-1);
  assert.equal(reply.content, "已通过 Rust 工具读取记忆。");
  assert.equal(reply.tokens, 39);
  assert(
    !reply.citations.some(
      (citation) => citation.id === thread.messages[0].sourceId,
    ),
  );
  assert(reply.citations.some((citation) => citation.id === note.id));
  assert(reply.matchedNodeIds.includes(note.id));
  assert(reply.activatedNodeIds.includes(note.id));
  const calls = modelRequests.filter(
    (request) =>
      request.model === "runtime-tool" &&
      !messageText(request.messages[0]).startsWith("将 source.text"),
  );
  assert.equal(calls.length, 3);
  assert(
    !JSON.stringify(calls[0].messages).includes("仅工具检索可得到的验收内容"),
  );
  assert(calls[1].messages.some((message) => message.role === "tool"));
  await memory("DELETE", `/nodes/${note.id}`);
  const removed = (await agent("GET", path)).messages.at(-1);
  assert.equal(removed.memoryStatus, "unavailable");
  assert(!removed.activatedNodeIds.includes(note.id));
  console.log(
    "Rust runtime, native memory tool, controlled credentials and graph activation checks passed",
  );
}
