import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import pg from "pg";

export async function verifyQueue({ memory, directory, provider }) {
  const admin = new pg.Pool({
    connectionString: process.env.AIO_TEST_DATABASE_URL,
  });
  try {
    const { source } = JSON.parse(
      await readFile(`${directory}/memory/memory-host.json`, "utf8"),
    );
    const { rows } = await admin.query(
      "SELECT schema_name FROM aio_plugin_host.database_bindings WHERE source_id=$1 AND tenant_id=$2",
      [source, "preview"],
    );
    const schema = rows[0].schema_name;
    assert.match(schema, /^p_[a-f0-9]+$/);
    const space = (
      await memory("POST", "/spaces", {
        title: "任务恢复验收",
        modelBinding: provider.id,
      })
    ).body;
    const capture = async (text, user = "developer") =>
      (
        await memory(
          "POST",
          "/capture",
          { requestId: randomUUID(), spaceId: space.id, text },
          user,
        )
      ).body;
    const claim = async () =>
      (
        await memory(
          "POST",
          "/tasks/claim",
          { spaceId: space.id },
          "developer",
          true,
        )
      ).body;
    const submit = (task, result) =>
      memory(
        "POST",
        `/tasks/${task.id}/submit`,
        { lease: task.lease, result },
        "developer",
        true,
      );
    const taskRow = async (id) =>
      (
        await admin.query(
          `SELECT * FROM ${schema}.plugin_memory_tasks WHERE id=$1`,
          [id],
        )
      ).rows[0];
    const wake = async (id) =>
      admin.query(
        `UPDATE ${schema}.plugin_memory_tasks SET available_at=0 WHERE id=$1`,
        [id],
      );
    const empty = { entries: [], relations: [] };

    const receipt = await capture("任务租约恢复资料");
    const lost = await claim();
    assert.equal(lost.id, receipt.id);
    await admin.query(
      `UPDATE ${schema}.plugin_memory_tasks SET lease_until=0 WHERE id=$1`,
      [lost.id],
    );
    const recovered = await claim();
    assert.equal(recovered.id, lost.id);
    assert.notEqual(recovered.lease, lost.lease);
    assert.equal((await submit(lost, empty)).status, 409);
    await memory(
      "POST",
      `/tasks/${recovered.id}/fail`,
      { lease: recovered.lease, code: "cancelled" },
      "developer",
      true,
    );
    assert.equal(Number((await taskRow(receipt.id)).attempts), 1);
    assert.equal((await taskRow(receipt.id)).state, "pending");
    for (let attempt = 2; attempt <= 5; attempt++) {
      await wake(receipt.id);
      const task = await claim();
      assert.equal(task.id, receipt.id);
      await memory(
        "POST",
        `/tasks/${task.id}/fail`,
        { lease: task.lease, code: "model_unavailable" },
        "developer",
        true,
      );
    }
    assert.equal((await taskRow(receipt.id)).state, "failed");
    assert.equal(
      (await memory("GET", `/sources/${receipt.id}`)).body.status,
      "failed",
    );
    await memory("POST", `/sources/${receipt.id}/retry`);
    const retried = await claim();
    assert.equal(Number((await taskRow(receipt.id)).attempts), 1);
    assert.equal((await submit(retried, empty)).body.status, "complete");

    await memory("POST", `/spaces/${space.id}/members`, {
      userId: "queue-editor",
      role: "EDITOR",
    });
    const revoked = await capture("撤权后不得继续整理", "queue-editor");
    const claimed = await claim();
    assert.equal(claimed.id, revoked.id);
    await memory("DELETE", `/spaces/${space.id}/members/queue-editor`);
    assert.equal((await submit(claimed, empty)).status, 403);
    assert.equal(await claim(), null);
    assert.equal((await taskRow(revoked.id)).state, "cancelled");
    assert.equal(
      (await memory("GET", `/sources/${revoked.id}`)).body.status,
      "failed",
    );

    const guarded = await capture("待校验的模型结果");
    const task = await claim();
    assert.equal(task.id, guarded.id);
    assert.equal(
      (
        await submit(task, {
          entries: [
            {
              draft: {
                title: "不应保存",
                content: "password: injected-model-secret",
              },
            },
          ],
        })
      ).status,
      400,
    );
    assert.equal(
      (
        await submit(task, {
          entries: [
            {
              draft: {
                title: "伪造引用",
                content: `[[secret:${"a".repeat(32)}]]`,
              },
            },
          ],
        })
      ).status,
      400,
    );
    assert.equal(
      (
        await submit(task, {
          entries: [
            {
              draft: { title: "错误归属", content: "无秘密" },
              secretIds: ["b".repeat(32)],
            },
          ],
        })
      ).status,
      400,
    );
    const another = (await memory("POST", "/spaces", { title: "另一个空间" }))
      .body;
    const foreign = (
      await memory("POST", `/nodes?spaceId=${another.id}`, {
        title: "其他空间条目",
        content: "不可越界",
      })
    ).body;
    assert.equal(
      (
        await submit(task, {
          entries: [
            {
              draft: { title: "不能覆盖" },
              existingId: foreign.id,
              baseVersion: foreign.version,
            },
          ],
        })
      ).status,
      404,
    );
    assert.equal(
      (await memory("GET", `/nodes/${foreign.id}`)).body.content,
      "不可越界",
    );
    const initial = {
      entries: [
        {
          draft: {
            title: "自动维护项目",
            kind: "PROJECT",
            content: "第一版事实",
            aliases: ["队列项目"],
          },
        },
      ],
    };
    assert.equal((await submit(task, initial)).body.status, "complete");
    assert.equal((await submit(task, initial)).body.status, "complete");
    const graph = (await memory("GET", `/graph?spaceId=${space.id}`)).body;
    const node = graph.nodes.find((node) => node.title === "自动维护项目");
    assert.equal(
      graph.nodes.filter(
        (entry) => entry.title === node.title && entry.kind === "PROJECT",
      ).length,
      1,
    );

    await capture("自动维护项目增加第二条事实");
    const next = await claim();
    assert(next.existing.some((entry) => entry.id === node.id));
    const revision = {
      entries: [
        {
          draft: {
            title: node.title,
            kind: "PROJECT",
            content: "第一版事实\n第二版事实",
            aliases: node.aliases,
          },
          existingId: node.id,
          baseVersion: node.version,
        },
      ],
    };
    assert.equal((await submit(next, revision)).body.status, "complete");
    const updated = (await memory("GET", `/nodes/${node.id}`)).body;
    assert.equal(updated.version, 2);
    assert(updated.content.includes("第一版事实"));
    const evidence = await admin.query(
      `SELECT source_version FROM ${schema}.plugin_memory_revision_sources rs JOIN ${schema}.plugin_memory_revisions r ON r.id=rs.revision_id WHERE r.node_id=$1 ORDER BY r.version`,
      [node.id],
    );
    assert.deepEqual(
      evidence.rows.map((row) => Number(row.source_version)),
      [1, 1],
    );
    await capture("自动维护项目存在事实矛盾");
    const uncertain = await claim();
    assert.equal(
      (
        await submit(uncertain, {
          ...revision,
          entries: revision.entries.map((entry) => ({
            ...entry,
            baseVersion: 2,
          })),
          needsReview: true,
        })
      ).body.status,
      "conflict",
    );
    assert.equal((await memory("GET", `/nodes/${node.id}`)).body.version, 2);
    assert.equal(
      (
        await memory("POST", `/sources/${uncertain.id}/resolve`, {
          accept: false,
        })
      ).body.status,
      "complete",
    );
  } finally {
    await admin.end();
  }
}
