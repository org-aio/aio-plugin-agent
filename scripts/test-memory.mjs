import assert from "node:assert/strict";
import { createServer } from "node:http";
import { spawn, execFileSync } from "node:child_process";
import { randomBytes, randomUUID } from "node:crypto";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { createInterface } from "node:readline";
import { resolve } from "node:path";
import pg from "pg";

const directory = process.env.AIO_MEMORY_TEST_DIRECTORY || "build/memory-test";
const memoryRoot = resolve(
  process.env.AIO_MEMORY_ROOT || "../aio-plugin-agent-memory",
);
execFileSync(process.execPath, ["scripts/setup-dev.mjs"], {
  env: { ...process.env, AIO_AGENT_DEV_DIRECTORY: directory },
  stdio: "inherit",
});
await mkdir(`${directory}/memory`, { recursive: true, mode: 0o700 });
const config = JSON.parse(await readFile(`${directory}/runtime.json`, "utf8"));
const bridgeToken = randomBytes(32).toString("hex");
const canary = `canary-${randomUUID()}`;
const modelRequests = [];
const failures = [];
const blockedRecalls = new Map();
let logs = "",
  child,
  backend,
  receive,
  bridgeGate;
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const servers = [];
async function listen(handler) {
  const server = createServer(handler);
  servers.push(server);
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return `http://127.0.0.1:${server.address().port}`;
}
async function stop(process, signal = "SIGTERM") {
  if (!process || process.exitCode !== null) return;
  await new Promise((resolve) => {
    process.once("exit", resolve);
    process.kill(signal);
  });
}
async function jsonBody(request) {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  return JSON.parse(Buffer.concat(chunks));
}
async function startMemory() {
  child = spawn(`${memoryRoot}/dev/target/release/aio-agent-memory-dev`, [], {
    cwd: memoryRoot,
    env: {
      ...process.env,
      AIO_MEMORY_DEV_DIRECTORY: resolve(directory, "memory"),
    },
    stdio: ["pipe", "pipe", "pipe"],
  });
  child.stderr.on("data", (bytes) => {
    logs += bytes;
  });
  let ready = false;
  createInterface({ input: child.stdout }).on("line", (line) => {
    const result = JSON.parse(line);
    if (result.ready) ready = true;
    else if (receive) {
      const complete = receive;
      receive = null;
      complete(result);
    }
  });
  for (let i = 0; !ready; i++) {
    assert(child.exitCode === null && i < 600, "Memory startup failed");
    await wait(100);
  }
}
let serial = Promise.resolve();
function memory(method, path, body = null, user = "developer", worker = false) {
  const operation = serial.then(async () => {
    const url = new URL(path, "http://local");
    const pending = new Promise((resolve) => {
      receive = resolve;
    });
    child.stdin.write(
      JSON.stringify({
        method,
        path: url.pathname,
        query: url.search.slice(1) || null,
        body:
          body === null ? [] : Array.from(Buffer.from(JSON.stringify(body))),
        userId: user,
        worker,
      }) + "\n",
    );
    const result = await pending;
    assert(!result.error, result.error);
    const decoded = {
      status: result.status,
      body: result.body.length ? JSON.parse(Buffer.from(result.body)) : null,
    };
    if (result.status >= 400)
      failures.push({
        method,
        path,
        status: result.status,
        error: decoded.body?.error,
      });
    return decoded;
  });
  serial = operation.catch(() => {});
  return operation;
}
const broker = await listen(async (req, res) => {
  try {
    assert.equal(req.headers.authorization, `Bearer ${bridgeToken}`);
    if (bridgeGate) await bridgeGate;
    const input = await jsonBody(req);
    assert.equal(input.tenantId, "preview");
    const target = new URL(input.path, "http://memory");
    const spaceId = target.searchParams.get("spaceId");
    if (target.pathname === "/route" && blockedRecalls.has(spaceId)) {
      blockedRecalls.set(spaceId, blockedRecalls.get(spaceId) + 1);
      throw new Error("测试中的检索服务不可用");
    }
    const result = await memory(
      input.method,
      input.path,
      input.body,
      input.userId,
      !input.interactive,
    );
    res
      .writeHead(200, { "content-type": "application/json" })
      .end(JSON.stringify(result));
  } catch {
    res.writeHead(503).end("{}");
  }
});
const upstream = await listen(async (req, res) => {
  const input = await jsonBody(req);
  modelRequests.push(input);
  const compiling = input.messages[0].content.startsWith("将 source.text");
  if (!compiling && input.model === "fail") {
    res.writeHead(503).end(canary);
    return;
  }
  if (!compiling && input.model === "broken") {
    res
      .writeHead(200, { "content-type": "text/event-stream" })
      .end("data: {invalid}\n\n");
    return;
  }
  if (
    !compiling &&
    (input.model === "slow" ||
      input.messages.at(-1).content.includes("停止生成验收"))
  ) {
    res.writeHead(200, { "content-type": "text/event-stream" });
    const timer = setInterval(
      () =>
        res.write(
          `data: ${JSON.stringify({ choices: [{ delta: { content: "已净化的流式输出。" } }] })}\n\n`,
        ),
      150,
    );
    res.on("close", () => clearInterval(timer));
    return;
  }
  let content = "已记录。";
  if (compiling) {
    const { source } = JSON.parse(input.messages.at(-1).content);
    content = JSON.stringify({
      entries: [
        {
          draft: {
            title: "测试项目",
            kind: "PROJECT",
            content: "项目资料已整理",
            tags: ["integration"],
          },
          secretIds: [],
        },
        {
          draft: {
            title: "测试账号",
            kind: "NOTE",
            content: source.text,
            tags: ["account"],
          },
          secretIds: source.secrets.map((secret) => secret.id),
        },
      ],
      relations: [
        {
          sourceIndex: 1,
          targetIndex: 0,
          relation: "属于",
          evidence: "同一份已净化来源",
        },
      ],
    });
  }
  res
    .writeHead(200, { "content-type": "text/event-stream" })
    .end(
      `data: ${JSON.stringify({ choices: [{ delta: { content } }] })}\n\ndata: [DONE]\n\n`,
    );
});
const port = Number(process.env.AIO_MEMORY_TEST_PORT || 4299),
  origin = `http://127.0.0.1:${port}`;
async function startAgent() {
  backend = spawn("target/debug/az-agent-server", [], {
    env: {
      ...process.env,
      AIO_AGENT_DATABASE_URL: config.databaseUrl,
      AIO_AGENT_MASTER_KEY: config.masterKey,
      AIO_AGENT_INGRESS_TOKEN: config.ingressToken,
      AIO_AGENT_ENDPOINTS: `${upstream}/v1`,
      AIO_AGENT_ALLOW_LOOPBACK: "1",
      AIO_AGENT_MEMORY_URL: broker,
      AIO_AGENT_MEMORY_TOKEN: bridgeToken,
      AIO_PLUGIN_PORT: String(port),
    },
    stdio: ["ignore", "ignore", "pipe"],
  });
  backend.stderr.on("data", (bytes) => {
    logs += bytes;
  });
  for (let i = 0; ; i++) {
    assert(backend.exitCode === null && i < 100, "Agent startup failed");
    try {
      if ((await fetch(`${origin}/health`)).ok) return;
    } catch {}
    await wait(100);
  }
}
async function agent(method, path, body, user = "developer", status = 200) {
  const response = await fetch(origin + path, {
    method,
    headers: {
      "content-type": "application/json",
      "x-aio-token": config.ingressToken,
      "x-aio-tenant-id": "preview",
      "x-aio-user-id": user,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  assert.equal(response.status, status, `${method} ${path}: unexpected status`);
  return text ? JSON.parse(text) : null;
}
async function eventually(read, predicate) {
  for (let i = 0; i < 150; i++) {
    const result = await read();
    if (predicate(result)) return result;
    await wait(200);
  }
  throw new Error("Memory workflow did not reach expected status");
}
const pool = new pg.Pool({ connectionString: config.databaseUrl });
async function verify() {
  await startMemory();
  await startAgent();
  const provider = await agent("POST", "/providers", {
    label: "memory-test",
    model: "memory-test",
    endpoint: `${upstream}/v1`,
    secret: "provider-test-key",
  });
  const space = await agent("POST", "/memory", {
    method: "POST",
    path: "/spaces",
    body: { title: `集成测试 ${randomUUID()}`, modelBinding: provider.id },
  });
  if (process.env.AIO_MEMORY_BROWSER_ONLY === "1") {
    const { verifyBrowser } = await import("./memory-browser.mjs");
    await verifyBrowser({
      directory,
      backendPort: port,
      space,
      provider,
      agent,
      canary,
    });
    console.log("Memory conversation and graph browser checks passed");
    return;
  }
  const conversation = await agent("POST", "/conversations", {
    title: "自动记忆验收",
    providerId: provider.id,
    spaceId: space.id,
  });
  const prompt = {
    requestId: randomUUID(),
    content: JSON.stringify({
      project: "测试项目",
      website: "https://example.test",
      username: "alice",
      password: canary,
      note: `备注中重复 ${canary}`,
    }),
  };
  const accepted = await agent(
    "POST",
    `/conversations/${conversation.id}/messages`,
    prompt,
  );
  assert(!JSON.stringify(accepted).includes(canary));
  assert(accepted.messages.at(-1).content.includes("已收下"));
  await agent("POST", `/conversations/${conversation.id}/messages`, prompt);
  await agent(
    "POST",
    `/conversations/${conversation.id}/messages`,
    { ...prompt, content: "不同内容" },
    "developer",
    400,
  );
  let thread = await eventually(
    () => agent("GET", `/conversations/${conversation.id}`),
    (value) => value.messages.at(-1)?.memoryStatus === "complete",
  );
  assert.equal(thread.messages.length, 2);
  assert(!JSON.stringify(thread).includes(canary));
  const sourceId = thread.messages[0].sourceId;
  const source = (await memory("GET", `/sources/${sourceId}`)).body;
  assert.equal(source.status, "complete");
  assert.equal(source.secrets.length, 1);
  let releaseBridge;
  bridgeGate = new Promise((resolve) => {
    releaseBridge = resolve;
  });
  try {
    const before = performance.now();
    const replay = await Promise.race([
      agent("POST", `/conversations/${conversation.id}/messages`, prompt),
      wait(1500).then(() => {
        throw new Error("Receipt waited for Memory");
      }),
    ]);
    assert(performance.now() - before < 1500);
    assert(replay.messages.at(-1).content.includes("已收下"));
  } finally {
    bridgeGate = null;
    releaseBridge();
  }
  const graph = (await memory("GET", `/graph?spaceId=${space.id}`)).body;
  assert.equal(graph.nodes.filter((node) => node.kind !== "SOURCE").length, 2);
  for (const value of [
    graph,
    source,
    (
      await memory("POST", `/context?spaceId=${space.id}`, {
        nodeIds: [sourceId],
        depth: 1,
      })
    ).body,
    modelRequests,
    logs,
  ])
    assert(
      !JSON.stringify(value).includes(canary),
      "Canary leaked into ordinary path",
    );
  const secret = source.secrets[0].id;
  assert.equal(
    (await memory("POST", `/secrets/${secret}/reveal`)).body.value,
    canary,
  );
  assert.equal(
    (await memory("POST", `/secrets/${secret}/reveal`, null, "developer", true))
      .status,
    403,
  );
  assert.equal(
    (await memory("GET", `/sources/${sourceId}`, null, "outsider")).status,
    403,
  );
  assert.equal(
    (
      await memory("POST", `/spaces/${space.id}/members`, {
        userId: "reader",
        role: "READER",
      })
    ).status,
    204,
  );
  assert.equal(
    (await memory("GET", `/sources/${sourceId}`, null, "reader")).status,
    200,
  );
  assert.equal(
    (await memory("POST", `/secrets/${secret}/reveal`, null, "reader")).status,
    403,
  );
  assert.equal(
    (await memory("POST", `/sources/${sourceId}/original`, null, "reader"))
      .status,
    403,
  );
  assert.equal(
    (
      await memory("PUT", `/secrets/${secret}/grants`, {
        userId: "reader",
        reveal: true,
        manage: false,
      })
    ).status,
    204,
  );
  assert.equal(
    (await memory("POST", `/secrets/${secret}/reveal`, null, "reader")).body
      .value,
    canary,
  );
  await memory("DELETE", `/spaces/${space.id}/members/reader`);
  assert.equal(
    (await memory("POST", `/secrets/${secret}/reveal`, null, "reader")).status,
    403,
  );
  await memory("POST", `/spaces/${space.id}/members`, {
    userId: "space-admin",
    role: "OWNER",
  });
  assert.equal(
    (await memory("POST", `/secrets/${secret}/reveal`, null, "space-admin"))
      .status,
    403,
  );
  assert.equal(
    (await memory("POST", `/sources/${sourceId}/original`, null, "space-admin"))
      .status,
    403,
  );
  const visibility = (
    await memory("POST", `/visibility?spaceId=${space.id}`, {
      nodeIds: [
        sourceId,
        ...Array.from({ length: 399 }, () => randomUUID().replaceAll("-", "")),
      ],
    })
  ).body;
  assert.deepEqual(visibility, [sourceId]);
  const persisted = await pool.query(
    "SELECT content,error,citations FROM agent_messages WHERE conversation_id=$1",
    [conversation.id],
  );
  assert(!JSON.stringify(persisted.rows).includes(canary));
  const { verifyRouting } = await import("./memory-routing-tests.mjs");
  await verifyRouting({
    agent,
    memory,
    eventually,
    modelRequests,
    canary,
    provider,
  });
  const { verifyGeneration } = await import("./memory-generation-tests.mjs");
  await verifyGeneration({
    agent,
    space,
    endpoint: `${upstream}/v1`,
    eventually,
    startAgent,
    stopAgent: (signal) => stop(backend, signal),
    pool,
    canary,
    blockedRecalls,
  });
  await stop(backend);
  await stop(child);
  await startMemory();
  await startAgent();
  thread = await agent("GET", `/conversations/${conversation.id}`);
  assert.equal(thread.messages.length, 2);
  assert.equal(
    (await memory("POST", `/secrets/${secret}/reveal`)).body.value,
    canary,
  );
  if (process.env.AIO_MEMORY_BROWSER === "1") {
    const { verifyBrowser } = await import("./memory-browser.mjs");
    await verifyBrowser({
      directory,
      backendPort: port,
      space,
      provider,
      agent,
      canary,
    });
  }
  await memory("DELETE", `/nodes/${sourceId}`);
  const deleted = (await memory("GET", `/graph?spaceId=${space.id}`)).body;
  assert(
    deleted.nodes.every(
      (node) => !graph.nodes.some((prior) => prior.id === node.id),
    ),
  );
  assert.equal((await memory("POST", `/secrets/${secret}/reveal`)).status, 404);
  const hidden = await agent("GET", `/conversations/${conversation.id}`);
  assert(
    hidden.messages.every((message) => message.memoryStatus === "unavailable"),
  );
  await stop(backend);
  const personal = (await memory("GET", "/spaces")).body.find(
    (space) => space.personal,
  );
  const unconfigured = (
    await memory("POST", "/capture", {
      requestId: randomUUID(),
      spaceId: personal.id,
      text: "尚未配置模型的资料",
    })
  ).body;
  assert.equal(unconfigured.status, "pending");
  assert.equal(
    (
      await memory(
        "POST",
        "/tasks/claim",
        { spaceId: personal.id },
        "developer",
        true,
      )
    ).body,
    null,
  );
  const quarantine = (
    await memory("POST", "/capture", {
      requestId: randomUUID(),
      spaceId: personal.id,
      text: "password: |\n  unlabelled multiline",
    })
  ).body;
  assert.equal(quarantine.status, "quarantined");
  const quarantinedRaw = "R".repeat(48);
  const held = (
    await memory("POST", "/capture", {
      requestId: randomUUID(),
      spaceId: personal.id,
      text: quarantinedRaw,
      origin: "chat",
      reference: "clarification-test",
    })
  ).body;
  assert.equal(held.status, "quarantined");
  await memory("POST", "/capture", {
    requestId: randomUUID(),
    spaceId: personal.id,
    text: "上一条是密码",
    origin: "chat",
    reference: "another-conversation",
    clarifies: held.id,
  });
  assert.equal(
    (await memory("GET", `/sources/${held.id}`)).body.status,
    "quarantined",
  );
  await memory("POST", "/capture", {
    requestId: randomUUID(),
    spaceId: personal.id,
    text: "上一条是密码",
    origin: "chat",
    reference: "clarification-test",
    clarifies: held.id,
  });
  const clarified = (await memory("GET", `/sources/${held.id}`)).body;
  assert.equal(clarified.status, "pending");
  assert(!clarified.text.includes(quarantinedRaw));
  assert.equal(
    (await memory("POST", `/secrets/${clarified.secrets[0].id}/reveal`)).body
      .value,
    quarantinedRaw,
  );
  const reviewSpace = (
    await memory("POST", "/spaces", {
      title: "修订验收",
      modelBinding: provider.id,
    })
  ).body;
  const route = (path) => `${path}?spaceId=${reviewSpace.id}`;
  const human = (
    await memory("POST", route("/nodes"), {
      title: "人工条目",
      content: "人工版本",
    })
  ).body;
  const input = {
    requestId: randomUUID(),
    spaceId: reviewSpace.id,
    text: "对人工条目的补充记录",
  };
  const received = (await memory("POST", "/capture", input)).body;
  assert.equal((await memory("POST", "/capture", input)).body.id, received.id);
  assert.equal(
    (await memory("POST", "/capture", { ...input, text: "不同内容" })).status,
    400,
  );
  const task = (
    await memory(
      "POST",
      "/tasks/claim",
      { spaceId: reviewSpace.id },
      "developer",
      true,
    )
  ).body;
  const proposal = {
    entries: [
      {
        draft: { title: human.title, content: "已核实的修订" },
        existingId: human.id,
        baseVersion: human.version,
        secretIds: [],
      },
    ],
    relations: [],
  };
  const submitted = await memory(
    "POST",
    `/tasks/${task.id}/submit`,
    { lease: task.lease, result: proposal },
    "developer",
    true,
  );
  assert.equal(submitted.body.status, "conflict");
  const changed = await memory(
    "POST",
    `/tasks/${task.id}/submit`,
    { lease: task.lease, result: { entries: [], relations: [] } },
    "developer",
    true,
  );
  assert.equal(changed.status, 409);
  assert.equal(
    (await memory("GET", `/nodes/${human.id}`)).body.content,
    "人工版本",
  );
  const resolved = await memory("POST", `/sources/${task.id}/resolve`, {
    accept: true,
    versions: { [human.id]: 1 },
  });
  assert.equal(resolved.body.status, "complete");
  const updated = (await memory("GET", `/nodes/${human.id}`)).body;
  assert.equal(updated.version, 2);
  assert.equal(updated.content, "已核实的修订");
  const rolled = (
    await memory("POST", `/nodes/${human.id}/rollback`, {
      version: 1,
      currentVersion: 2,
    })
  ).body;
  assert.equal(rolled.version, 3);
  assert.equal(rolled.content, "人工版本");
  const aliasNode = (
    await memory("POST", route("/nodes"), {
      title: "独立主题",
      content: "无关键词正文",
      aliases: ["代号星辰"],
    })
  ).body;
  assert(
    (
      await memory("POST", route("/recall"), { query: "代号星辰", limit: 8 })
    ).body.nodes.some((node) => node.id === aliasNode.id),
  );
  const { verifyQueue } = await import("./memory-queue-tests.mjs");
  await verifyQueue({ memory, directory, provider });
  assert(!logs.includes(canary));
  await writeFile(
    `${directory}/report.json`,
    JSON.stringify(
      {
        capture: true,
        idempotency: true,
        wiki: true,
        graph: true,
        aliases: true,
        noSecretLeak: true,
        separateSecretGrants: true,
        revocation: true,
        restart: true,
        sourceDeletion: true,
        clarification: true,
        conflictReview: true,
        rollback: true,
        leaseRecovery: true,
        retryExhaustion: true,
        cancellation: true,
        upstreamFailure: true,
        localRouting: true,
        persistedActivation: true,
        modelCalls: modelRequests.length,
      },
      null,
      2,
    ),
  );
  console.log(
    "Memory integration passed: capture, queue, wiki, graph, secret isolation, grants, revocation, restart, source deletion, no-model intake, quarantine, conflict review, rollback",
  );
}
try {
  await verify();
} catch (error) {
  console.error(
    JSON.stringify({ modelCalls: modelRequests.length, failures }).replaceAll(
      canary,
      "[protected]",
    ),
  );
  throw error;
} finally {
  await stop(backend);
  await stop(child);
  await pool.end();
  await Promise.all(
    servers.map((server) => new Promise((resolve) => server.close(resolve))),
  );
}
