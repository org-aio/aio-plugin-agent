import { createServer } from "node:net";
import { spawn, execFileSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { resolve } from "node:path";

const root = process.cwd();
const memoryRoot = resolve(
  process.env.AIO_MEMORY_ROOT || "../aio-plugin-agent-memory",
);
const directory = resolve(
  process.env.AIO_AGENT_DEV_DIRECTORY || ".local/memory-integration",
);
if (!process.env.AIO_TEST_DATABASE_URL)
  throw new Error("需要独立开发数据库 AIO_TEST_DATABASE_URL");
execFileSync(process.execPath, ["scripts/setup-dev.mjs"], {
  env: { ...process.env, AIO_AGENT_DEV_DIRECTORY: directory },
  stdio: "inherit",
});
const children = [];
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
let stopping = false;
async function available(preferred) {
  for (let port = preferred; port < preferred + 100; port++) {
    const server = createServer();
    const free = await new Promise((resolve) => {
      server.once("error", () => resolve(false));
      server.listen(port, "127.0.0.1", () => server.close(() => resolve(true)));
    });
    if (free) return port;
  }
  throw new Error("没有可用的开发端口");
}
async function shutdown() {
  if (stopping) return;
  stopping = true;
  await Promise.all(
    children.map((child) =>
      child.exitCode !== null
        ? undefined
        : new Promise((resolve) => {
            const timeout = setTimeout(() => child.kill("SIGKILL"), 5000);
            child.once("exit", () => {
              clearTimeout(timeout);
              resolve();
            });
            child.kill("SIGTERM");
          }),
    ),
  );
}
async function launch(cwd, env, url) {
  const child = spawn(process.execPath, ["scripts/preview.mjs"], {
    cwd,
    env: { ...process.env, ...env },
    stdio: "inherit",
  });
  children.push(child);
  child.once("exit", () => {
    if (!stopping) void shutdown();
  });
  for (let attempt = 0; attempt < 300; attempt++) {
    if (child.exitCode !== null) throw new Error("开发服务启动失败");
    try {
      if ((await fetch(url)).ok) return;
    } catch {}
    await pause(100);
  }
  throw new Error("开发服务启动超时");
}
try {
  const memoryPort = await available(
    Number(process.env.AIO_MEMORY_PORT || 4191),
  );
  const token = randomBytes(32).toString("hex");
  const memoryUrl = `http://127.0.0.1:${memoryPort}`;
  await launch(
    memoryRoot,
    {
      PORT: String(memoryPort),
      AIO_MEMORY_DEV_DIRECTORY: resolve(directory, "memory"),
      AIO_MEMORY_BRIDGE_TOKEN: token,
    },
    memoryUrl,
  );
  const agentPort = await available(Number(process.env.PORT || 4192));
  const backendPort = await available(
    Math.max(agentPort + 1, Number(process.env.AIO_PLUGIN_PORT || 4193)),
  );
  const agentUrl = `http://127.0.0.1:${agentPort}`;
  await launch(
    root,
    {
      PORT: String(agentPort),
      AIO_PLUGIN_PORT: String(backendPort),
      AIO_AGENT_DEV_CONFIG: resolve(directory, "runtime.json"),
      AIO_AGENT_MEMORY_URL: `${memoryUrl}/broker`,
      AIO_AGENT_MEMORY_TOKEN: token,
      AIO_AGENT_ALLOW_LOOPBACK: "1",
    },
    agentUrl,
  );
  console.log(`Agent: ${agentUrl}\nMemory: ${memoryUrl}`);
  for (const signal of ["SIGINT", "SIGTERM"])
    process.on(signal, () => void shutdown());
} catch (error) {
  await shutdown();
  throw error;
}
