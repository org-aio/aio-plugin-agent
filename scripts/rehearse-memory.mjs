import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { randomBytes, createDecipheriv } from "node:crypto";
import { readFile, mkdir, copyFile, writeFile } from "node:fs/promises";
import { createInterface } from "node:readline";
import { resolve } from "node:path";
import pg from "pg";

process.umask(0o077);
const sourceDirectory = resolve(
  process.env.AIO_MEMORY_TEST_DIRECTORY || "build/memory-test",
);
const sourceUrl = new URL(process.env.AIO_TEST_DATABASE_URL);
assert(
  ["127.0.0.1", "localhost", "[::1]"].includes(sourceUrl.hostname),
  "此演练脚本仅接受独立的本地验收数据库",
);
const name = `aio_memory_restore_${randomBytes(6).toString("hex")}`;
const directory = resolve("build", name);
await mkdir(`${directory}/memory`, { recursive: true, mode: 0o700 });
const config = JSON.parse(
  await readFile(`${sourceDirectory}/runtime.json`, "utf8"),
);
const memoryConfig = JSON.parse(
  await readFile(`${sourceDirectory}/memory/memory-host.json`, "utf8"),
);
const admin = new pg.Pool({ connectionString: sourceUrl.href });
let restored, agentPool, child;
try {
  const bindings = await admin.query(
    "SELECT schema_name FROM aio_plugin_host.database_bindings WHERE source_id=$1 AND tenant_id=$2",
    [memoryConfig.source, "preview"],
  );
  const schema = bindings.rows[0].schema_name;
  assert.match(schema, /^p_[a-f0-9]+$/);
  const sourceAgent = new pg.Pool({ connectionString: config.databaseUrl });
  const expected = await sourceAgent.query(
    "SELECT count(*)::integer AS messages FROM agent_messages",
  );
  await sourceAgent.end();
  execFileSync(
    "pg_dump",
    [
      "--format=custom",
      "--file",
      `${directory}/database.dump`,
      "--dbname",
      sourceUrl.href,
    ],
    { stdio: ["ignore", "ignore", "inherit"] },
  );
  await admin.query(`CREATE DATABASE ${name} TEMPLATE template0`);
  const targetUrl = new URL(sourceUrl);
  targetUrl.pathname = `/${name}`;
  execFileSync(
    "pg_restore",
    [
      "--exit-on-error",
      "--dbname",
      targetUrl.href,
      `${directory}/database.dump`,
    ],
    { stdio: ["ignore", "ignore", "inherit"] },
  );
  const agentUrl = new URL(config.databaseUrl);
  agentUrl.pathname = `/${name}`;
  await writeFile(
    `${directory}/runtime.json`,
    JSON.stringify({ ...config, databaseUrl: agentUrl.href }),
    { mode: 0o600 },
  );
  await copyFile(
    `${sourceDirectory}/memory/memory-host.json`,
    `${directory}/memory/memory-host.json`,
  );
  restored = new pg.Pool({ connectionString: targetUrl.href });
  agentPool = new pg.Pool({ connectionString: agentUrl.href });
  assert.deepEqual(
    (
      await agentPool.query(
        "SELECT count(*)::integer AS messages FROM agent_messages",
      )
    ).rows,
    expected.rows,
  );
  const provider = (
    await agentPool.query(
      "SELECT id,tenant_id,user_id,secret FROM agent_providers WHERE secret IS NOT NULL LIMIT 1",
    )
  ).rows[0];
  const cipher = createDecipheriv(
    "aes-256-gcm",
    Buffer.from(config.masterKey, "base64"),
    provider.secret.subarray(0, 12),
  );
  cipher.setAAD(
    Buffer.from(
      JSON.stringify([provider.tenant_id, provider.user_id, provider.id]),
    ),
  );
  cipher.setAuthTag(provider.secret.subarray(-16));
  assert.equal(
    Buffer.concat([
      cipher.update(provider.secret.subarray(12, -16)),
      cipher.final(),
    ]).toString(),
    "provider-test-key",
  );
  const secret = (
    await restored.query(
      `SELECT id FROM ${schema}.plugin_memory_secrets WHERE label=$1 LIMIT 1`,
      ["密码"],
    )
  ).rows[0];
  assert(secret, "源目录必须来自完整记忆验收");
  child = spawn(
    resolve(
      "../aio-plugin-agent-memory/dev/target/release/aio-agent-memory-dev",
    ),
    [],
    {
      cwd: resolve("../aio-plugin-agent-memory"),
      env: {
        ...process.env,
        AIO_TEST_DATABASE_URL: targetUrl.href,
        AIO_MEMORY_DEV_DIRECTORY: `${directory}/memory`,
      },
      stdio: ["pipe", "pipe", "ignore"],
    },
  );
  let ready = false,
    receive;
  createInterface({ input: child.stdout }).on("line", (line) => {
    const result = JSON.parse(line);
    if (result.ready) ready = true;
    else if (receive) {
      const done = receive;
      receive = null;
      done(result);
    }
  });
  for (let i = 0; !ready; i++) {
    assert(i < 600 && child.exitCode === null, "恢复后的 Component 启动失败");
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const invoke = async (worker) => {
    const pending = new Promise((resolve) => {
      receive = resolve;
    });
    child.stdin.write(
      JSON.stringify({
        method: "POST",
        path: `/secrets/${secret.id}/reveal`,
        body: [],
        userId: "developer",
        worker,
      }) + "\n",
    );
    return pending;
  };
  assert.equal((await invoke(true)).status, 403);
  const revealed = await invoke(false);
  assert.equal(revealed.status, 200);
  assert.equal(JSON.parse(Buffer.from(revealed.body)).value, "R".repeat(48));
  const report = {
    database: name,
    agentMessages: expected.rows[0].messages,
    agentScopedRole: true,
    agentKeyRestored: true,
    componentKeyRestored: true,
    workerSecretDenied: true,
    production: false,
  };
  await writeFile(`${directory}/report.json`, JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report));
} finally {
  if (child?.exitCode === null)
    await new Promise((resolve) => {
      child.once("exit", resolve);
      child.stdin.end();
    });
  await agentPool?.end();
  await restored?.end();
  await admin.end();
}
