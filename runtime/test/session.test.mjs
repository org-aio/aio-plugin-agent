import assert from "node:assert/strict";
import test from "node:test";
import { runSession } from "../session.mjs";

function response(delta, reason = "stop") {
  return new Response(
    [
      `data: ${JSON.stringify({ choices: [{ index: 0, delta, finish_reason: null }] })}\n\n`,
      `data: ${JSON.stringify({ choices: [{ index: 0, delta: {}, finish_reason: reason }], usage: { prompt_tokens: 10, completion_tokens: 3, total_tokens: 13 } })}\n\n`,
      "data: [DONE]\n\n",
    ].join(""),
    { headers: { "content-type": "text/event-stream" } },
  );
}

const request = (tools = []) => ({
  version: 1,
  model: "runtime-test",
  tools,
  messages: [
    { role: "system", content: "只使用已净化资料回答。" },
    { role: "user", content: "第一条已净化消息" },
    { role: "assistant", content: "已收下。" },
    { role: "user", content: "请分析会议安排" },
  ],
});

test("Pi restores sanitized history and streams a model reply", async () => {
  const events = [];
  const calls = [];
  await runSession(request(), {
    send: (event) => events.push(event),
    async fetch(input, options) {
      const value = new Request(input, options);
      calls.push(JSON.parse(await value.text()));
      assert.equal(value.url, "https://aio.invalid/v1/chat/completions");
      assert.equal(value.headers.get("authorization"), "Bearer managed-by-aio");
      return response({ content: "会议在周三。" });
    },
    tool() {
      assert.fail("Tools must be disabled");
    },
  });
  assert.equal(calls.length, 1);
  assert.equal(calls[0].messages[0].content, "只使用已净化资料回答。");
  assert(
    calls[0].messages.some((message) => message.content === "第一条已净化消息"),
  );
  assert(!calls[0].tools?.length);
  assert.equal(
    events
      .filter((event) => event.type === "text")
      .map((event) => event.text)
      .join(""),
    "会议在周三。",
  );
  assert.equal(events.at(-1).tokens, 13);
});

test("Pi executes its native memory extension and continues the agent loop", async () => {
  const events = [];
  let calls = 0;
  let toolCalls = 0;
  await runSession(request(["memory_search"]), {
    send: (event) => events.push(event),
    async fetch(input, options) {
      const body = JSON.parse(await new Request(input, options).text());
      calls++;
      assert.deepEqual(
        body.tools.map((tool) => tool.function.name),
        ["memory_search"],
      );
      if (calls === 1)
        return response(
          {
            tool_calls: [
              {
                index: 0,
                id: "lookup_1",
                type: "function",
                function: {
                  name: "memory_search",
                  arguments: JSON.stringify({ query: "会议" }),
                },
              },
            ],
          },
          "tool_calls",
        );
      assert(
        body.messages.some(
          (message) =>
            message.role === "tool" && message.content.includes("周三"),
        ),
      );
      return response({ content: "根据记忆，会议在周三。" });
    },
    async tool(name, args) {
      toolCalls++;
      assert.equal(name, "memory_search");
      assert.deepEqual(args, { query: "会议" });
      return {
        context: "会议在周三",
        citations: [{ id: "source-1", title: "会议" }],
      };
    },
  });
  assert.equal(calls, 2);
  assert.equal(toolCalls, 1);
  assert.equal(events.at(-1).tokens, 26);
});

test("Pi upstream failures never become successful replies", async () => {
  await assert.rejects(
    runSession(request(), {
      send() {},
      fetch: async () =>
        new Response("untrusted upstream error", { status: 503 }),
    }),
    /Agent response failed/,
  );
});

test("Pi rejects streams without a completion reason", async () => {
  await assert.rejects(
    runSession(request(), {
      send() {},
      fetch: async () =>
        new Response(
          'data: {"choices":[{"delta":{"content":"partial"}}]}\n\ndata: [DONE]\n\n',
          { headers: { "content-type": "text/event-stream" } },
        ),
    }),
    /Agent response failed/,
  );
});
