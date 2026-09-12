import { execFileSync } from "node:child_process";
import { readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

const lock = JSON.parse(readFileSync("graph/source.lock.json", "utf8"));
if (!/^[a-f0-9]{40}$/.test(lock.revision))
  throw new Error("Invalid graph revision");
const repo = resolve(process.env.AIO_GRAPH_SOURCE || "build/graph-source");
if (!process.env.AIO_GRAPH_SOURCE) {
  mkdirSync(repo, { recursive: true });
  execFileSync("git", ["init", "-q", repo]);
  try {
    execFileSync(
      "git",
      ["-C", repo, "cat-file", "-e", `${lock.revision}^{commit}`],
      { stdio: "ignore" },
    );
  } catch {
    execFileSync(
      "git",
      ["-C", repo, "fetch", "--depth=1", lock.git, lock.revision],
      { stdio: "inherit" },
    );
  }
}
const working = process.argv.includes("--working-tree");
if (working && !process.env.AIO_GRAPH_SOURCE)
  throw new Error("Working tree requires AIO_GRAPH_SOURCE");
for (const file of lock.files) {
  const source = `${lock.root}/${file}`;
  const bytes = working
    ? readFileSync(resolve(repo, source))
    : execFileSync("git", ["-C", repo, "show", `${lock.revision}:${source}`]);
  const output = resolve(
    "graph/src/site/addzero/component/knowledge_graph",
    file,
  );
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, bytes);
}
console.log(
  `Graph dependency: ${working ? "local development" : lock.revision}`,
);
