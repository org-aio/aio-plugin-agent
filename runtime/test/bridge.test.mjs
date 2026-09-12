import assert from "node:assert/strict";
import test from "node:test";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const modules = fileURLToPath(new URL("../../node_modules", import.meta.url));

for (const mode of ["complete", "disconnect"]) {
  test(
    `Runtime process ${mode} terminates its session`,
    { timeout: 15000 },
    async () => {
      const child = spawn(
        process.execPath,
        [
          "--permission",
          `--allow-fs-read=${root}`,
          `--allow-fs-read=${modules}`,
          `${root}main.mjs`,
        ],
        {
          cwd: root,
          env: { PI_OFFLINE: "1" },
          stdio: ["pipe", "pipe", "pipe"],
        },
      );
      const events = [];
      let diagnostics = "";
      child.stderr.on("data", (data) => {
        diagnostics += data;
      });
      const closed = new Promise((resolve) =>
        child.once("exit", (code) => resolve(code)),
      );
      const input = createInterface({ input: child.stdout });
      const send = (event) => child.stdin.write(JSON.stringify(event) + "\n");
      input.on("line", (line) => {
        const event = JSON.parse(line);
        events.push(event);
        if (event.type !== "fetch") return;
        if (mode === "disconnect") {
          child.stdin.end();
          return;
        }
        send({ type: "response", id: event.id, status: 200 });
        send({
          type: "chunk",
          id: event.id,
          data: Buffer.from(
            'data: {"choices":[{"index":0,"delta":{"content":"hello"},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n',
          ).toString("base64"),
        });
        send({ type: "end", id: event.id });
      });
      try {
        send({
          type: "start",
          version: 1,
          model: "runtime-test",
          tools: [],
          messages: [
            { role: "system", content: "Only answer this test." },
            { role: "user", content: "hello" },
          ],
        });
        const code = await closed;
        assert.equal(code, 0, diagnostics);
        assert(events.some((event) => event.type === "fetch"));
        assert(
          events.some(
            (event) => event.type === (mode === "complete" ? "done" : "error"),
          ),
        );
        if (mode === "complete")
          assert.equal(
            events
              .filter((event) => event.type === "text")
              .map((event) => event.text)
              .join(""),
            "hello",
          );
      } finally {
        child.kill();
        input.close();
      }
    },
  );
}
