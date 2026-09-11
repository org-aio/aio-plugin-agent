import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { resolve } from "node:path";
const lock = JSON.parse(readFileSync("sdk/web/source.json", "utf8"));
for (const [file, digest] of Object.entries(lock.files)) {
  const hash = (path) =>
    createHash("sha256").update(readFileSync(path)).digest("hex");
  if (hash(`sdk/web/${file}`) !== digest)
    throw new Error(`SDK snapshot mismatch: ${file}`);
  if (
    process.env.AIO_PLATFORM &&
    hash(resolve(process.env.AIO_PLATFORM, "sdk/web", file)) !== digest
  )
    throw new Error(`Platform SDK changed: ${file}`);
}
console.log("AIO Web SDK snapshot verified");
