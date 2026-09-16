import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const image = process.argv[2];
if (!/^sha256:[a-f0-9]{64}$/.test(image ?? ""))
  throw new Error("请传入已构建的 runtime 镜像完整摘要");

const tests = fileURLToPath(new URL("../runtime/test", import.meta.url));
execFileSync(
  "docker",
  [
    "run",
    "--rm",
    "--network=none",
    "--read-only",
    "--user=65532:65532",
    "--cap-drop=ALL",
    "--security-opt=no-new-privileges",
    "--pids-limit=128",
    "--mount",
    `type=bind,source=${tests},target=/app/runtime/test,readonly`,
    image,
    "/usr/bin/env",
    "-i",
    "PATH=/usr/local/bin:/usr/bin:/bin",
    "PI_OFFLINE=1",
    "/usr/local/bin/node",
    "--test",
    "/app/runtime/test/bridge.test.mjs",
  ],
  { stdio: "inherit" },
);
