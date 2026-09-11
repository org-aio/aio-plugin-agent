import assert from "node:assert/strict";
import { createServer } from "node:http";
import { spawn, execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import pg from "pg";

const directory = "build/service-test";
execFileSync(process.execPath, ["scripts/setup-dev.mjs"], {
  env: { ...process.env, AIO_AGENT_DEV_DIRECTORY: directory },
  stdio: "inherit",
});
const config = JSON.parse(await readFile(`${directory}/runtime.json`, "utf8"));
const requests = [];
let cancelled = 0;
const upstream = createServer(async (req, res) => {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const body = JSON.parse(Buffer.concat(chunks).toString());
  requests.push({ body, authorization: req.headers.authorization });
  if (body.model === "fail") {
    res.writeHead(503).end("test-secret must not reach the client");
    return;
  }
  res.writeHead(200, { "content-type": "text/event-stream" });
  if (body.model === "broken") {
    res.end("data: {invalid}\n\n");
    return;
  }
  const parts = [
    "Rust 后端已收到",
    "这条测试消息。",
    "\n这是协议测试输出，",
    "不是模型推理。",
  ];
  let index = 0,
    done = false;
  const timer = setInterval(
    () => {
      if (index < parts.length)
        res.write(
          `data: ${JSON.stringify({ choices: [{ delta: { content: parts[index++] } }] })}\n\n`,
        );
      else if (body.model !== "slow") {
        res.end(
          `data: ${JSON.stringify({ choices: [], usage: { total_tokens: 42 } })}\n\ndata: [DONE]\n\n`,
        );
        done = true;
        clearInterval(timer);
      }
    },
    body.model === "slow" ? 300 : 140,
  );
  res.on("close", () => {
    clearInterval(timer);
    if (!done) cancelled++;
  });
});
await new Promise((resolve) => upstream.listen(0, "127.0.0.1", resolve));
const endpoint = `http://127.0.0.1:${upstream.address().port}/v1`;
const probe = createServer();
await new Promise((resolve) => probe.listen(0, "127.0.0.1", resolve));
const port = probe.address().port;
await new Promise((resolve) => probe.close(resolve));
const origin = `http://127.0.0.1:${port}`;
const user = `test-${randomUUID()}`;
let backend;
async function start() {
  backend = spawn("target/debug/az-agent-server", [], {
    stdio: ["ignore", "ignore", "pipe"],
    env: {
      ...process.env,
      AIO_AGENT_DATABASE_URL: config.databaseUrl,
      AIO_AGENT_MASTER_KEY: config.masterKey,
      AIO_AGENT_INGRESS_TOKEN: config.ingressToken,
      AIO_AGENT_ENDPOINTS: endpoint,
      AIO_AGENT_ALLOW_LOOPBACK: "1",
      AIO_PLUGIN_PORT: String(port),
    },
  });
  let stderr = "";
  backend.stderr.on("data", (d) => (stderr += d));
  for (let i = 0; i < 100; i++) {
    if (backend.exitCode !== null) throw new Error(stderr);
    try {
      if ((await fetch(`${origin}/health`)).ok) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error("Backend startup timeout");
}
async function stop(signal = "SIGTERM") {
  if (!backend || backend.exitCode !== null) return;
  await new Promise((resolve) => {
    backend.once("exit", resolve);
    backend.kill(signal);
  });
}
async function call(
  method,
  path,
  body,
  identity = { tenant: "test", user },
  status = 200,
) {
  const response = await fetch(origin + path, {
    method,
    headers: {
      "content-type": "application/json",
      "x-aio-token": config.ingressToken,
      "x-aio-tenant-id": identity.tenant,
      "x-aio-user-id": identity.user,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  assert.equal(response.status, status, text);
  return text ? JSON.parse(text) : null;
}
async function finished(
  id,
  predicate = (value) => value.status !== "generating",
) {
  for (let i = 0; i < 100; i++) {
    const thread = await call("GET", `/conversations/${id}`);
    const message = thread.messages.at(-1);
    if (predicate(message)) return message;
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error("Generation did not finish");
}
const conversationIds = [],
  providerIds = [];
const pool = new pg.Pool({ connectionString: config.databaseUrl });
try {
  await start();
  assert.equal((await fetch(`${origin}/settings`)).status, 401);
  const createProvider = async (model) => {
    const p = await call("POST", "/providers", {
      label: model,
      model,
      endpoint,
      secret: "test-secret",
    });
    providerIds.push(p.id);
    return p;
  };
  const provider = await createProvider("stream");
  assert(provider.hasSecret);
  assert(!JSON.stringify(provider).includes("test-secret"));
  await call(
    "POST",
    "/providers",
    { label: "invalid", model: "x", endpoint: "https://unapproved.example/v1" },
    undefined,
    400,
  );
  const create = async (p) => {
    const c = await call("POST", "/conversations", {
      title: "协议验收",
      providerId: p.id,
    });
    conversationIds.push(c.id);
    return c;
  };
  const conversation = await create(provider);
  const path = `/conversations/${conversation.id}`;
  await call("GET", path, undefined, { tenant: "other", user }, 404);
  await call("GET", path, undefined, { tenant: "test", user: "other" }, 404);
  const prompt = {
    requestId: randomUUID(),
    content: "Test prompt, no user data",
  };
  await call("POST", `${path}/messages`, prompt);
  await call("POST", `${path}/messages`, prompt);
  await call(
    "POST",
    `${path}/messages`,
    { requestId: randomUUID(), content: "overlap" },
    undefined,
    409,
  );
  const partial = await finished(
    conversation.id,
    (m) => m.status === "generating" && m.content.length > 0,
  );
  assert(partial.content.length > 0);
  const complete = await finished(conversation.id);
  assert.equal(complete.status, "complete");
  assert.equal(complete.tokens, 42);
  assert(complete.content.includes("不是模型推理"));
  assert.equal(requests.length, 1);
  assert.equal(requests[0].authorization, "Bearer test-secret");
  const encrypted = (
    await pool.query("SELECT secret FROM agent_providers WHERE id=$1", [
      provider.id,
    ])
  ).rows[0].secret;
  assert(!encrypted.includes(Buffer.from("test-secret")));
  const slow = await create(await createProvider("slow"));
  await call("POST", `/conversations/${slow.id}/messages`, {
    requestId: randomUUID(),
    content: "Cancel test",
  });
  await finished(slow.id, (m) => m.content.length > 0);
  await call("POST", `/conversations/${slow.id}/cancel`);
  assert.equal((await finished(slow.id)).status, "cancelled");
  assert(cancelled > 0);
  for (const model of ["fail", "broken"]) {
    const c = await create(await createProvider(model));
    await call("POST", `/conversations/${c.id}/messages`, {
      requestId: randomUUID(),
      content: "Error test",
    });
    const failure = await finished(c.id);
    assert.equal(failure.status, "failed");
    assert(!JSON.stringify(failure).includes("test-secret"));
  }
  await call("DELETE", `/providers/${provider.id}`, undefined, undefined, 409);
  await call("POST", `/conversations/${slow.id}/messages`, {
    requestId: randomUUID(),
    content: "Restart test",
  });
  await finished(slow.id, (m) => m.content.length > 0);
  await stop("SIGKILL");
  await start();
  assert.equal((await finished(slow.id)).status, "interrupted");
  assert.equal(
    (await call("GET", path)).messages.at(-1).content,
    complete.content,
  );
  await pool.query(
    "UPDATE agent_messages SET status='generating' WHERE id=$1",
    [complete.id],
  );
  const recovered = (await call("GET", path)).messages.at(-1);
  assert.equal(recovered.status, "interrupted");
  assert.equal(recovered.content, complete.content);
  await call("POST", `${path}/messages`, {
    requestId: randomUUID(),
    content: "After recovery",
  });
  assert.equal((await finished(conversation.id)).status, "complete");
  await mkdir("test-results", { recursive: true });
  const report = {
    postgres: true,
    partialOutput: true,
    idempotency: true,
    ownership: true,
    cancellation: true,
    upstreamFailure: true,
    encryptedSecrets: true,
    restartPersistence: true,
    interruptedRecovery: true,
    orphanRecovery: true,
    liveModel: false,
  };
  await writeFile(
    "test-results/service-report.json",
    JSON.stringify(report, null, 2),
  );
  console.log(JSON.stringify(report, null, 2));
} finally {
  for (const id of conversationIds)
    await pool.query(
      "DELETE FROM agent_conversations WHERE id=$1 AND user_id=$2",
      [id, user],
    );
  for (const id of providerIds)
    await pool.query("DELETE FROM agent_providers WHERE id=$1 AND user_id=$2", [
      id,
      user,
    ]);
  await stop();
  await pool.end();
  await new Promise((resolve) => upstream.close(resolve));
}
