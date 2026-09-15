import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, readdir, rm, utimes } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { run } from '../cli/src/notes/pipeline.mjs';

export async function verifyLocalNotes({ agent, eventually, provider, canary, modelRequests }) {
  const root = await mkdtemp(join(tmpdir(), 'aio-memory-local-'));
  try {
    const space = await agent('POST', '/memory', { method: 'POST', path: '/spaces', body: { title: 'CLI 笔记验收', modelBinding: provider.id } });
    const notes = join(root, 'notes');
    await mkdir(join(notes, 'original'), { recursive: true });
    await mkdir(join(notes, 'duplicate'));
    const old = new Date(Date.now() - 3600_000);
    for (const directory of ['original', 'duplicate']) {
      const path = join(notes, directory, '星桥.md');
      await writeFile(path, '星桥例会时间：周四 17:30。'); await utimes(path, old, old);
    }
    const secretPath = join(notes, '账户.md');
    await writeFile(secretPath, JSON.stringify({ account: 'example', password: canary })); await utimes(secretPath, old, old);
    const client = {
      profile: { origin: 'http://127.0.0.1', userId: 'developer', tenantId: 'preview' },
      connect: async () => {}, agent,
      memory: (method, path, body = null) => agent('POST', '/memory', { method, path, body }),
    };
    const config = { roots: [notes], output: join(root, 'wiki'), spaceId: space.id, modelBinding: provider.id, model: provider.model, cleanup: 'duplicates', batchSize: 10 };
    const options = { trashRoot: join(root, 'trash') };
    const first = await run(config, client, options);
    assert.equal(first.failed, 0);
    const state = JSON.parse(await readFile(join(config.output, 'state.json')));
    const sourceIds = [...new Set(Object.values(state.records).flatMap(record => record.sources))];
    assert.equal(sourceIds.length, 2, '重复文件共用收件来源');
    for (const source of sourceIds) await eventually(() => client.memory('GET', `/sources/${source}?spaceId=${space.id}`), value => value.status === 'complete');
    const second = await run(config, client, options);
    assert.equal(second.failed, 0);
    assert.equal(second.trashed, 1);
    assert.equal((await readdir(options.trashRoot)).length, 1);
    assert((await readFile(secretPath, 'utf8')).includes(canary), '秘密原件必须保留');
    const graph = JSON.parse(await readFile(join(config.output, 'graph.json')));
    assert(graph.edges.some(edge => edge.relation === '属于'), 'Wiki 导出包含模型整理的知识关系');
    for (const path of await readdir(config.output, { recursive: true })) {
      if (!/\.(md|json)$/.test(path)) continue;
      assert(!(await readFile(join(config.output, path), 'utf8')).includes(canary), '本地产物不得包含测试密码');
    }
    assert(!JSON.stringify(modelRequests).includes(canary), '模型请求不得包含测试密码');
    const conversation = await agent('POST', '/conversations', { title: 'CLI 资料询问', spaceId: space.id });
    const before = modelRequests.length;
    await agent('POST', `/conversations/${conversation.id}/messages`, { requestId: randomUUID(), content: '查找 星桥例会' });
    const thread = await eventually(() => agent('GET', `/conversations/${conversation.id}`), value => value.messages.at(-1)?.status === 'complete');
    const answer = thread.messages.at(-1);
    assert(answer.content.includes('17:30'));
    assert(answer.activatedNodeIds.some(id => graph.nodes.some(node => node.id === id)));
    assert.equal(modelRequests.length, before, '明确查找不消耗前台模型请求');
    console.log('CLI 笔记链路通过：真实收件、去重、Pi 整理、wiki 校验、密码隔离、可恢复清理及 AIO 对话召回');
  } finally { await rm(root, { recursive: true, force: true }); }
}
