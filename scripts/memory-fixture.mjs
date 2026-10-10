import assert from "node:assert/strict";
import { createServer } from "node:http";
import { createServer as portServer } from "node:net";
import { spawn } from "node:child_process";
import { randomBytes, createCipheriv, createDecipheriv } from "node:crypto";
import { mkdir, readFile, readdir, writeFile, rm } from "node:fs/promises";
import { resolve, join } from "node:path";
import { createInterface } from "node:readline";
import pg from "pg";

// 联调运行当前原生 Memory；只有宿主加密通道由独立测试密钥模拟。
const root = process.cwd();
const directory = resolve(process.env.AIO_MEMORY_DEV_DIRECTORY);
await mkdir(directory, { recursive: true, mode: 0o700 });
const configPath = join(directory, "memory-host.json");
let config;
try {
  config = JSON.parse(await readFile(configPath, "utf8"));
} catch (error) {
  if (error.code !== "ENOENT") throw error;
}
const adminUrl = new URL(process.env.AIO_TEST_DATABASE_URL);
assert(
  ["localhost", "127.0.0.1"].includes(adminUrl.hostname),
  "仅使用本地隔离测试数据库",
);
const admin = new pg.Client({ connectionString: adminUrl.href });
await admin.connect();
try {
  if (!config) {
    const schema = `p_${randomBytes(16).toString("hex")}`;
    const password = randomBytes(32).toString("hex");
    const role = `r_${schema}`;
    await admin.query("BEGIN");
    await admin.query(
      `CREATE ROLE ${role} LOGIN PASSWORD '${password}' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOINHERIT`,
    );
    await admin.query(`CREATE SCHEMA ${schema}`);
    await admin.query(`SET LOCAL search_path TO ${schema}`);
    for (const file of (await readdir(join(root, "backend/migrations")))
      .filter((name) => name.endsWith(".sql"))
      .sort()) {
      await admin.query(
        await readFile(join(root, "backend/migrations", file), "utf8"),
      );
    }
    await admin.query(`GRANT USAGE ON SCHEMA ${schema} TO ${role}`);
    await admin.query(
      `GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA ${schema} TO ${role}`,
    );
    await admin.query(
      `GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA ${schema} TO ${role}`,
    );
    await admin.query(`ALTER ROLE ${role} SET search_path TO ${schema}`);
    await admin.query("COMMIT");
    const url = new URL(adminUrl);
    url.username = role;
    url.password = password;
    config = {
      schema,
      databaseUrl: url.href,
      key: randomBytes(32).toString("base64"),
      token: randomBytes(32).toString("hex"),
    };
    await writeFile(configPath, JSON.stringify(config), { mode: 0o600 });
  }
} finally {
  await admin.end();
}
const key = Buffer.from(config.key, "base64");
const socket = join(directory, "crypto.sock");
await rm(socket, { force: true });
const broker = createServer(async (request, response) => {
  try {
    assert.equal(request.headers["x-aio-token"], config.token);
    assert.equal(request.method, "POST");
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const input = JSON.parse(Buffer.concat(chunks));
    const bytes = Buffer.from(input.value, "base64");
    let output;
    if (request.url === "/cryptography/seal") {
      const nonce = randomBytes(12);
      const cipher = createCipheriv("aes-256-gcm", key, nonce);
      cipher.setAAD(Buffer.from(input.purpose));
      const ciphertext = Buffer.concat([cipher.update(bytes), cipher.final()]);
      output = Buffer.concat([nonce, cipher.getAuthTag(), ciphertext]);
    } else {
      assert.equal(request.url, "/cryptography/open");
      const cipher = createDecipheriv(
        "aes-256-gcm",
        key,
        bytes.subarray(0, 12),
      );
      cipher.setAAD(Buffer.from(input.purpose));
      cipher.setAuthTag(bytes.subarray(12, 28));
      output = Buffer.concat([
        cipher.update(bytes.subarray(28)),
        cipher.final(),
      ]);
    }
    const body = JSON.stringify({ value: output.toString("base64") });
    response
      .writeHead(200, {
        "content-type": "application/json",
        "content-length": Buffer.byteLength(body),
      })
      .end(body);
  } catch {
    response.writeHead(403).end();
  }
});
await new Promise((resolve) => broker.listen(socket, resolve));
const probe = portServer();
await new Promise((resolve) => probe.listen(0, "127.0.0.1", resolve));
const port = probe.address().port;
await new Promise((resolve) => probe.close(resolve));
const child = spawn(
  process.env.AIO_MEMORY_SERVER || join(root, "target/debug/az-memory-server"),
  [],
  {
    cwd: root,
    env: {
      ...process.env,
      AIO_MEMORY_DATABASE_URL: config.databaseUrl,
      AIO_MEMORY_INGRESS_TOKEN: config.token,
      AIO_MEMORY_BROKER_SOCKET: socket,
      AIO_MEMORY_BROKER_TOKEN: config.token,
      AIO_PLUGIN_PORT: String(port),
    },
    stdio: ["ignore", "ignore", "inherit"],
  },
);
child.on("error", (error) => {
  console.error(error.message);
  process.exit(1);
});
for (let i = 0; ; i++) {
  assert(child.exitCode === null && i < 300, "原生 Memory 启动失败");
  try {
    if ((await fetch(`http://127.0.0.1:${port}/health`)).ok) break;
  } catch {}
  await new Promise((resolve) => setTimeout(resolve, 100));
}
async function invoke(input) {
  const path = input.path + (input.query ? `?${input.query}` : "");
  assert(path.startsWith("/") && !path.startsWith("//"));
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: input.method,
    headers: {
      "content-type": "application/json",
      "x-aio-token": config.token,
      "x-aio-tenant-id": "preview",
      "x-aio-user-id": input.userId,
      "x-aio-context": input.worker ? "service:agent-test" : "interactive-test",
    },
    body: ["GET", "HEAD"].includes(input.method)
      ? undefined
      : Buffer.from(input.body),
    redirect: "error",
    signal: AbortSignal.timeout(30000),
  });
  return {
    status: response.status,
    body: [...new Uint8Array(await response.arrayBuffer())],
  };
}

// 本地预览复用同一原生夹具；这里只提供授权桥，不模拟 Memory 业务行为。
let preview;
if (process.env.AIO_MEMORY_BRIDGE_PORT) {
  const bridgeToken = process.env.AIO_MEMORY_BRIDGE_TOKEN;
  assert(bridgeToken?.length >= 32, "预览桥需要独立访问凭据");
  preview = createServer(async (request, response) => {
    try {
      if (request.method === "GET" && request.url === "/health") {
        response.writeHead(200).end("ok");
        return;
      }
      if (request.headers.authorization !== `Bearer ${bridgeToken}`) {
        response.writeHead(403).end();
        return;
      }
      assert(request.method === "POST" && request.url === "/broker");
      let size = 0;
      const chunks = [];
      for await (const chunk of request) {
        size += chunk.length;
        assert(size <= 3 * 1024 * 1024, "预览请求过大");
        chunks.push(chunk);
      }
      const input = JSON.parse(Buffer.concat(chunks));
      assert(input.tenantId === "preview" && typeof input.userId === "string");
      assert(typeof input.interactive === "boolean");
      assert(
        typeof input.path === "string" &&
          input.path.startsWith("/") &&
          !input.path.startsWith("//"),
      );
      const target = new URL(input.path, "http://memory");
      const result = await invoke({
        method: input.method,
        path: target.pathname,
        query: target.search.slice(1),
        body: input.body == null ? [] : Buffer.from(JSON.stringify(input.body)),
        userId: input.userId,
        worker: !input.interactive,
      });
      response.writeHead(200, { "content-type": "application/json" }).end(
        JSON.stringify({
          status: result.status,
          body: result.body.length
            ? JSON.parse(Buffer.from(result.body))
            : null,
        }),
      );
    } catch {
      response.writeHead(400).end("{}");
    }
  });
  await new Promise((resolve) =>
    preview.listen(
      Number(process.env.AIO_MEMORY_BRIDGE_PORT),
      "127.0.0.1",
      resolve,
    ),
  );
}
console.log(JSON.stringify({ ready: true }));
const lines = createInterface({ input: process.stdin });
async function shutdown() {
  lines.close();
  if (child.exitCode === null) {
    await new Promise((resolve) => {
      child.once("exit", resolve);
      child.kill("SIGTERM");
    });
  }
  await new Promise((resolve) => broker.close(resolve));
  if (preview) await new Promise((resolve) => preview.close(resolve));
  process.exit(0);
}
process.once("SIGTERM", shutdown);
process.once("SIGINT", shutdown);
for await (const line of lines) {
  try {
    console.log(JSON.stringify(await invoke(JSON.parse(line))));
  } catch (error) {
    console.log(JSON.stringify({ error: error.message }));
  }
}
await shutdown();
