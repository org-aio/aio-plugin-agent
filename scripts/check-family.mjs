import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
const parent = JSON.parse(readFileSync("family.json", "utf8"));
assert.equal(parent.repository, "aio-plugin-agent");
const names = new Set();
for (const child of parent.children) {
  assert.match(child.feature, /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/);
  assert.equal(child.repository, `${parent.repository}-${child.feature}`);
  assert.equal(new URL(child.git).pathname, `/org-aio/${child.repository}.git`);
  assert(!names.has(child.repository));
  names.add(child.repository);
}
if (process.argv[2]) {
  const child = JSON.parse(
    readFileSync(`${process.argv[2]}/family.json`, "utf8"),
  );
  assert.equal(child.parent, parent.repository);
  assert.equal(child.repository, `${child.parent}-${child.feature}`);
  assert(names.has(child.repository));
}
console.log("Agent subplugin naming verified");
