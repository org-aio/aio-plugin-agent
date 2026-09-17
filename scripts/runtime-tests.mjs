import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";

const title = "运行时工具专用条目";

export function messageText(message) {
  return typeof message.content === "string"
    ? message.content
    : message.content
        .filter((part) => part.type === "input_text")
        .map((part) => part.text)
        .join("\n");
}

export function runtimeModelResponse(input, response, authorization) {
  if (input.model !== "runtime-tool") return false;
  assert.equal(authorization, "Bearer runtime-provider-key");
  assert.deepEqual(
    input.tools.map((tool) => tool.name),
    ["request_user_input", "memory_search", "skill_list", "skill_read"],
  );
  const results = input.input.filter(
    (message) => message.type === "function_call_output",
  );
  if (results.length) assert(results[0].output.includes("没有找到相关资料"));
  const complete = results.length === 2;
  const output = complete
    ? [
        {
          type: "message",
          role: "assistant",
          content: [
            { type: "output_text", text: "已通过 Rust 工具读取记忆。" },
          ],
        },
      ]
    : [
        {
          type: "function_call",
          call_id: `memory_lookup_${results.length}`,
          name: "memory_search",
          arguments: JSON.stringify({
            query: results.length
              ? title
              : "zzqf16be774e18d493083ece7f493beab90e",
          }),
        },
      ];
  if (complete)
    assert(results[1].output.includes("仅工具检索可得到的验收内容"));
  response
    .writeHead(200, { "content-type": "text/event-stream" })
    .end(responseEvents(output, 13));
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
      !messageText(request.input[0]).startsWith("将 source.text"),
  );
  assert.equal(calls.length, 3);
  assert(
    !JSON.stringify(calls[0].input).includes("仅工具检索可得到的验收内容"),
  );
  assert(
    calls[1].input.some((message) => message.type === "function_call_output"),
  );
  await memory("DELETE", `/nodes/${note.id}`);
  const removed = (await agent("GET", path)).messages.at(-1);
  assert.equal(removed.memoryStatus, "unavailable");
  assert(!removed.activatedNodeIds.includes(note.id));
  console.log(
    "Rust runtime, native memory tool, controlled credentials and graph activation checks passed",
  );
}

export function responseEvents(output, tokens = 0) {
  return `event: response.completed\ndata: ${JSON.stringify({ type: "response.completed", response: { status: "completed", output, usage: { total_tokens: tokens } } })}\n\n`;
}
