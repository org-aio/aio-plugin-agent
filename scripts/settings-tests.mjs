import assert from "node:assert/strict";
export async function verifySettings({ agent, pool }) {
  const key = "search-configuration-test-key";
  assert.deepEqual((await agent("GET", "/settings")).webSearch, {
    enabled: false,
    hasSecret: false,
  });
  await agent("PUT", "/tools/web-search", { enabled: true }, "developer", 400);
  assert.deepEqual(
    await agent("PUT", "/tools/web-search", { enabled: true, secret: key }),
    { enabled: true, hasSecret: true },
  );
  const row = (
    await pool.query(
      "SELECT secret FROM agent_tool_settings WHERE tenant_id='preview' AND user_id='developer' AND tool='web_search'",
    )
  ).rows[0];
  assert(Buffer.isBuffer(row.secret) && !row.secret.includes(Buffer.from(key)));
  assert(!JSON.stringify(await agent("GET", "/settings")).includes(key));
  assert.deepEqual(
    (await agent("GET", "/settings", undefined, "outsider")).webSearch,
    { enabled: false, hasSecret: false },
  );
  assert.deepEqual(
    await agent("PUT", "/tools/web-search", { enabled: false }),
    { enabled: false, hasSecret: true },
  );
  assert.deepEqual(
    await agent("PUT", "/tools/web-search", { enabled: false, secret: "" }),
    { enabled: false, hasSecret: false },
  );
  console.log(
    "Plugin settings encryption, identity isolation and clear checks passed",
  );
}
