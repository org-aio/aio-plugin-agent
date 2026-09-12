import { createInterface } from "node:readline";

export function createBridge(start) {
  const pending = new Map();
  let sequence = 0;
  let started = false;
  const send = (message) =>
    process.stdout.write(JSON.stringify(message) + "\n");
  const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
  input.on("line", (line) => {
    try {
      if (Buffer.byteLength(line) > 256000) throw new Error("Protocol limit");
      const event = JSON.parse(line);
      if (event.type === "start" && !started && event.version === 1) {
        started = true;
        void start(event)
          .then(
            () => send({ type: "done" }),
            () => send({ type: "error" }),
          )
          .finally(() => {
            input.close();
            process.stdin.destroy();
          });
        return;
      }
      const request = pending.get(event.id);
      if (!request) throw new Error("Unexpected response");
      if (event.type === "response") {
        const stream = new ReadableStream({
          start(controller) {
            request.controller = controller;
          },
        });
        request.resolve(
          new Response(stream, {
            status: event.status,
            headers: { "content-type": "text/event-stream" },
          }),
        );
      } else if (event.type === "chunk")
        request.controller.enqueue(Buffer.from(event.data, "base64"));
      else if (event.type === "end") {
        request.controller.close();
        pending.delete(event.id);
      } else if (event.type === "tool_result") {
        request.resolve(event.value);
        pending.delete(event.id);
      } else throw new Error("Unexpected response");
    } catch {
      send({ type: "error" });
      process.exitCode = 1;
      input.close();
      process.stdin.destroy();
    }
  });
  input.on("close", () => {
    for (const request of pending.values()) {
      request.reject(new Error("Bridge closed"));
      request.controller?.error(new Error("Bridge closed"));
    }
    pending.clear();
  });
  const request = (event) =>
    new Promise((resolve, reject) => {
      const id = ++sequence;
      pending.set(id, { resolve, reject });
      send({ ...event, id });
    });
  return {
    send,
    async fetch(input, options) {
      const value = new Request(input, options);
      if (
        value.method !== "POST" ||
        value.url !== "https://aio.invalid/v1/chat/completions"
      )
        throw new Error("Endpoint not authorized");
      value.signal.throwIfAborted();
      return request({
        type: "fetch",
        url: value.url,
        body: JSON.parse(await value.text()),
      });
    },
    tool(name, args) {
      return request({ type: "tool", name, arguments: args });
    },
  };
}
