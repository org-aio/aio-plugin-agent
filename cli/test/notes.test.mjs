import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, rm, utimes, symlink, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { digest } from '../src/session/storage.mjs';
import { readNote, fileNotes, chunks } from '../src/notes/files.mjs';
import { trashVerified } from '../src/notes/cleanup.mjs';
import { exportSource } from '../src/memory/wiki.mjs';
import { run, readiness } from '../src/notes/pipeline.mjs';
import { snapshot } from '../src/notes/apple.mjs';

const space = 'a'.repeat(32), nodeId = 'b'.repeat(32), sourceId = 'c'.repeat(32);
const old = new Date(Date.now() - 3600_000);
async function fixture(fn) {
  const root = await mkdtemp(join(tmpdir(), 'aio-agent-notes-'));
  try { await fn(root); } finally { await rm(root, { recursive: true, force: true }); }
}

test('扫描忽略链接和编辑器资源，完整原文先隔离，不拆散凭据字段', () => fixture(async root => {
  const notes = join(root, 'notes'); await mkdir(notes);
  await writeFile(join(notes, 'note.md'), '📝 中文\n'.repeat(3000));
  await symlink(join(notes, 'note.md'), join(notes, 'alias.md'));
  await mkdir(join(notes, '.obsidian')); await writeFile(join(notes, '.obsidian', 'config.md'), 'ignore');
  const found = []; const report = { skipped: 0 };
  for await (const note of fileNotes([notes], join(root, 'wiki'), report)) found.push(note);
  assert.equal(found.length, 1);
  assert.equal(chunks(found[0]).join(''), `# note.md\n\n${found[0].text}`);
  assert(!chunks(found[0]).some(part => part.includes('�')));
  const credential = { title: '凭据', text: '笔记\n'.repeat(3000) + 'password:\ncredential-value' };
  assert.equal(chunks(credential).length, 1);
  const structured = { title: '结构化账户.md', text: '{"password":"test-secret"}' };
  assert.equal(chunks(structured)[0], structured.text);
  assert.throws(() => chunks({ title: '大文件', text: 'x'.repeat(100_000) }), /未拆分原文/);
}));

test('Apple 采集使用纯文本，跳过共享、锁定和最近删除且不读取它们的正文', () => {
  const note = (folder = 'Notes', shared = false, locked = false) => ({
    container: () => ({ name: () => folder, shared: () => shared }),
    passwordProtected: () => locked, shared: () => shared,
    modificationDate: () => old, id: () => 'note', name: () => '测试', attachments: () => [],
    plaintext: () => { assert.equal(folder, 'Notes'); assert(!shared && !locked); return 'password: test-value'; },
    body: () => { throw new Error('不应读取 HTML'); },
  });
  const app = { accounts: { byId: selected => {
    assert.equal(selected, 'selected-account');
    return { notes: () => [note(), note('最近删除'), note('Notes', true), note('Notes', false, true)] };
  } } };
  const result = snapshot(['notes', 'selected-account'], app);
  assert.equal(result.filter(value => value.skipped).length, 3);
  assert.equal(result[0].body, 'password: test-value');
});

test('只移动已核对且未改变的副本，保留附件和秘密', () => fixture(async root => {
  const one = join(root, 'one'), two = join(root, 'two'); await mkdir(one); await mkdir(two);
  const a = join(one, 'note.md'), b = join(two, 'note.md');
  await writeFile(a, '已整理知识'); await writeFile(b, '已整理知识');
  const note = await readNote(b), proofs = [{ verified: true, hasSecrets: false }], trash = join(root, 'trash');
  assert.equal(await trashVerified(note, [{ verified: false }], 'duplicates', a, trash), null);
  assert.equal(await trashVerified(note, [{ verified: true, hasSecrets: true }], 'duplicates', a, trash), null);
  assert.equal(await trashVerified({ ...note, attachments: true }, proofs, 'duplicates', a, trash), null);
  const moved = await trashVerified(note, proofs, 'duplicates', a, trash);
  assert.equal(await readFile(moved, 'utf8'), '已整理知识');
  assert.equal(await readFile(a, 'utf8'), '已整理知识');
  await assert.rejects(stat(b), { code: 'ENOENT' });
  await writeFile(b, '新内容'); const before = await readNote(b); await writeFile(b, '并发编辑');
  assert.equal(await trashVerified(before, proofs, 'archived', null, trash), null);
  assert.equal(await readFile(b, 'utf8'), '并发编辑');
}));

function memoryClient() {
  const captures = new Map(); let evidence = true, pending = false;
  return {
    profile: { origin: 'https://example.com', userId: 'user', tenantId: 'tenant' }, captures,
    connect: async () => {}, agent: async () => ({ providers: [{ id: 'binding', model: 'test-model' }] }),
    setEvidence(value) { evidence = value; }, setPending(value) { pending = value; },
    async memory(method, path, body) {
      const route = path.split('?')[0];
      if (route === '/spaces') return [{ id: space, role: 'OWNER', modelBinding: 'binding' }];
      if (route === '/capture') { captures.set(body.requestId, body.text); return { id: sourceId, spaceId: space, status: 'pending' }; }
      if (route === `/sources/${sourceId}`) return { id: sourceId, spaceId: space, text: '净化原文完整副本', status: pending ? 'pending' : 'complete', secrets: [] };
      if (route.endsWith('/edges')) return [{ id: 'edge', source: sourceId, target: nodeId, relation: '来源', evidence: '净化来源' }];
      if (route.endsWith('/sources')) return evidence ? [{ id: sourceId }] : [];
      if (route === `/nodes/${nodeId}`) return { id: nodeId, title: 'Wiki 条目', content: '整理后的知识', version: 1 };
      throw new Error('未预期调用');
    },
  };
}

test('wiki 导出核对来源依据，保存逐字净化副本和版本', () => fixture(async root => {
  const client = memoryClient();
  const result = await exportSource(client, sourceId, space, root);
  assert(result.verified);
  assert.equal(digest(await readFile(join(root, 'sources', sourceId + '.md'))), result.sourceDigest);
  assert((await readFile(join(root, 'entries', sourceId, nodeId + '.md'), 'utf8')).includes('整理后的知识'));
  client.setEvidence(false);
  await assert.rejects(exportSource(client, sourceId, space, root), /来源依据/);
  client.setPending(true);
  assert.equal((await exportSource(client, sourceId, space, root)).verified, false);
  client.setPending(false); client.setEvidence(true);
  const request = client.memory.bind(client); let version = 1;
  client.memory = async (...args) => {
    const result = await request(...args);
    return args[1].split('?')[0] === `/nodes/${nodeId}` ? { ...result, version: version++ } : result;
  };
  await assert.rejects(exportSource(client, sourceId, space, root), /wiki 版本发生变化/);
}));

test('重跑不重复收件、后续批次继续推进，错误模型不会收件', () => fixture(async root => {
  const notes = join(root, 'notes'); await mkdir(notes);
  for (const name of ['a', 'b']) { const path = join(notes, name + '.md'); await writeFile(path, name); await utimes(path, old, old); }
  const client = memoryClient();
  const config = { roots: [notes], output: join(root, 'wiki'), spaceId: space, modelBinding: 'binding', model: 'test-model', cleanup: 'none', batchSize: 1 };
  await assert.rejects(run({ ...config, model: 'wrong-model' }, client), /模型不一致/);
  assert.equal(client.captures.size, 0);
  assert.equal((await run(config, client)).captured, 1);
  assert.equal((await run(config, client)).captured, 1);
  assert.equal((await run(config, client)).captured, 0);
  assert.equal(client.captures.size, 2);
  assert((await readFile(join(root, 'wiki', 'index.md'), 'utf8')).includes('Wiki 条目'));
  assert.equal(JSON.parse(await readFile(join(root, 'wiki', 'graph.json'))).nodes.length, 2);
}));

test('收件提交后丢失响应可安全重试，未完成前不清理', () => fixture(async root => {
  const notes = join(root, 'notes'); await mkdir(notes);
  const path = join(notes, '重试.md'); await writeFile(path, '原始知识'); await utimes(path, old, old);
  const client = memoryClient(), request = client.memory.bind(client); let fail = true;
  client.memory = async (...args) => {
    const result = await request(...args);
    if (args[1] === '/capture' && fail) { fail = false; throw new Error('模拟响应丢失'); }
    return result;
  };
  const config = { roots: [notes], output: join(root, 'wiki'), spaceId: space, modelBinding: 'binding', model: 'test-model', cleanup: 'archived' };
  const options = { trashRoot: join(root, 'trash') };
  assert.equal((await run(config, client, options)).failed, 1);
  assert.equal(client.captures.size, 1);
  assert.equal(await readFile(path, 'utf8'), '原始知识');
  client.setPending(true);
  assert.equal((await run(config, client, options)).pending, 1);
  assert.equal(client.captures.size, 1);
  assert.equal(await readFile(path, 'utf8'), '原始知识');
  client.setPending(false);
  assert.equal((await run(config, client, options)).trashed, 1);
  assert.equal(client.captures.size, 1);
}));

test('缺少模型或登录时只报告就绪状态，不读取笔记', () => fixture(async root => {
  const status = await readiness({ profile: join(root, 'missing.json'), roots: [], cleanup: 'none' });
  assert.equal(status.configured, false);
  assert.deepEqual(status.missing, ['spaceId', 'modelBinding', 'model', 'login']);
}));
