import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { privateWrite } from '../src/session/storage.mjs';
import { appleNotes, appleKey, snapshot } from '../src/notes/apple.mjs';
import { captureChanges, watch, watchOptions, watchReadiness, watchState } from '../src/notes/watch.mjs';
import { launchProperties } from '../src/notes/launch.mjs';

const spaceId = 'a'.repeat(32);
const profile = { version: 1, origin: 'https://example.com', userId: 'owner', tenantId: 'workspace', cookie: 'aio_session=fixture' };
const normalize = text => text.replace(/\r\n/g, '\n').replace(/^[ \t\r\n]+|[ \t\r\n]+$/g, '');
const execute = promisify(execFile);

async function fixture(fn) {
  const root = await mkdtemp(join(tmpdir(), 'aio-notes-watch-'));
  try {
    const config = { version: 1, roots: [], cleanup: 'none', output: join(root, 'wiki'), profile: join(root, 'session.json'),
      spaceId, appleAccounts: ['selected-account'], watch: { intervalMs: 1000, settleMs: 1000 } };
    await privateWrite(config.profile, JSON.stringify(profile));
    await fn(root, config);
  } finally { await rm(root, { recursive: true, force: true }); }
}

function server() {
  const sources = new Map(), requests = new Map(), calls = [];
  return {
    sources, requests, calls, profile, close: async () => {},
    async memory(method, path, body) {
      if (path === '/spaces') return [{ id: spaceId, role: 'OWNER', modelBinding: null }];
      assert.equal(method, 'POST'); assert.equal(path, '/capture'); assert.equal(body.deduplicate, true);
      assert.equal(body.origin, 'import'); assert.equal(body.spaceId, spaceId);
      assert.match(body.requestId, /^[a-f0-9-]{36}$/);
      calls.push(body);
      const content = normalize(body.text);
      const previous = requests.get(body.requestId);
      if (previous) assert.equal(previous, content);
      requests.set(body.requestId, content);
      if (!sources.has(content)) sources.set(content, { id: (sources.size + 1).toString(16).padStart(32, '0'), spaceId, status: 'pending' });
      return sources.get(content);
    },
  };
}

const note = (text = '新增想法', extra = {}) => ({ account: 'selected-account', key: 'local-id', title: '随手记', text,
  kind: 'apple', modified: 1000, attachments: false, ...extra });
const collect = (...notes) => async function* () { yield* notes; };

test('新增笔记秒级稳定后入库，不等模型、15 分钟或 wiki 整理', () => fixture(async (root, config) => {
  const client = server(), { state, path } = await watchState(config, client);
  const options = { collect: collect(note()), now: 5000 };
  assert.equal((await captureChanges(config, client, state, path, options)).pending, 1);
  assert.equal(client.calls.length, 0);
  assert.equal((await captureChanges(config, client, state, path, { ...options, now: 6000 })).captured, 1);
  assert.equal(client.sources.size, 1);
  assert.equal((await captureChanges(config, client, state, path, { ...options, now: 7000 })).unchanged, 1);
  assert.equal(client.calls.length, 1);
  assert.equal((await stat(path)).mode & 0o777, 0o600);
}));

test('编辑期间防抖，修改入库为新内容；不覆盖已保存记录、不传播删除', () => fixture(async (root, config) => {
  const client = server(), { state, path } = await watchState(config, client);
  const tick = (text, now) => captureChanges(config, client, state, path, { collect: collect(note(text)), now });
  await tick('a', 5000); await tick('ab', 5500);
  assert.equal((await tick('abc', 6000)).pending, 1);
  assert.equal(client.sources.size, 0);
  assert.equal((await tick('abc', 7000)).captured, 1);
  assert.equal((await tick('abc\r\n', 8000)).unchanged, 1);
  assert.equal((await tick('abc 修改', 9000)).pending, 1);
  assert.equal((await tick('abc 修改', 10_000)).captured, 1);
  assert.equal(client.sources.size, 2);
  await captureChanges(config, client, state, path, { collect: collect(), now: 11_000 });
  assert.equal(client.sources.size, 2);
}));

test('稳定等待使用本机观察时间，不被另一台 Mac 的未来修改时间阻塞', () => fixture(async (root, config) => {
  const client = server(), item = await watchState(config, client);
  const values = collect(note('时钟偏差', { modified: 100_000 }));
  await captureChanges(config, client, item.state, item.path, { collect: values, now: 5000 });
  assert.equal((await captureChanges(config, client, item.state, item.path, { collect: values, now: 6000 })).captured, 1);
}));

test('两台设备独立 hash 状态和随机请求 ID，服务端复用同一来源', () => fixture(async (root, config) => {
  const client = server();
  const first = await watchState(config, client);
  const second = await watchState({ ...config, output: join(root, 'mini-wiki') }, client);
  for (const item of [first, second]) {
    await captureChanges(config, client, item.state, item.path, { collect: collect(note('双机笔记')), now: 5000 });
    await captureChanges(config, client, item.state, item.path, { collect: collect(note('双机笔记')), now: 6000 });
  }
  assert.equal(client.calls.length, 2);
  assert.notEqual(client.calls[0].requestId, client.calls[1].requestId);
  assert.equal(client.sources.size, 1);
  const key = appleKey('selected-account', 'local-id');
  assert.equal(first.state.records[key].sourceId, second.state.records[key].sourceId);
  const restarted = await watchState(config, client);
  await captureChanges(config, client, restarted.state, restarted.path, { collect: collect(note('双机笔记')), now: 7000 });
  assert.equal(client.calls.length, 2);
}));

test('提交后响应丢失，断点重启复用随机请求 ID 且退避重试', () => fixture(async (root, config) => {
  const client = server(), request = client.memory.bind(client);
  let fail = true;
  client.memory = async (...args) => {
    const source = await request(...args);
    if (fail && args[1] === '/capture') { fail = false; throw new Error('响应丢失'); }
    return source;
  };
  let item = await watchState(config, client);
  const text = 'password: watch-canary';
  await captureChanges(config, client, item.state, item.path, { collect: collect(note(text)), now: 5000 });
  assert.equal((await captureChanges(config, client, item.state, item.path, { collect: collect(note(text)), now: 6000 })).failed, 1);
  item = await watchState(config, client);
  assert.equal((await captureChanges(config, client, item.state, item.path, { collect: collect(note(text)), now: 7000 })).skipped, 1);
  assert.equal((await captureChanges(config, client, item.state, item.path, { collect: collect(note(text)), now: 9000 })).captured, 1);
  assert.equal(client.calls[0].requestId, client.calls[1].requestId);
  assert.equal(client.sources.size, 1);
  const persisted = await readFile(item.path, 'utf8');
  assert(!persisted.includes('watch-canary'));
  assert(!persisted.includes('随手记'));
  assert(!persisted.includes(profile.cookie));
  await assert.rejects(watchState(config, { profile: { ...profile, userId: 'different' } }), /其他用户或空间/);
}));

test('超额、空白、附件笔记保留原件且不会阻塞其他笔记', () => fixture(async (root, config) => {
  const client = server(), item = await watchState(config, client);
  const values = collect(note('x'.repeat(100_000), { key: 'large' }), note('', { key: 'empty' }), note('有附件的正文', { key: 'attachment', attachments: true }));
  const first = await captureChanges(config, client, item.state, item.path, { collect: values, now: 5000 });
  assert.equal(first.oversized, 1); assert.equal(first.skipped, 1);
  const second = await captureChanges(config, client, item.state, item.path, { collect: values, now: 6000 });
  assert.equal(second.captured, 1); assert.equal(second.withAttachments, 1);
  assert.equal(client.sources.size, 1);
  assert(!JSON.stringify(client.calls).includes('xxxxxxxxxx'));
}));

test('已保存且修改时间不变时只读元数据，不再读取正文；仅读取显式账号', async () => {
  const calls = [];
  const records = { [appleKey('selected-account', 'stable')]: { modified: 1000, savedAt: 5000 } };
  const report = { skipped: 0, unchanged: 0 };
  const read = async (operation, account, selected) => {
    calls.push(operation); assert.equal(account, 'selected-account');
    if (operation === 'metadata') return [{ id: 'stable', modified: 1000 }, { id: 'new', modified: 2000 }, { skipped: true }];
    assert.deepEqual(selected, ['new']);
    return [{ id: 'new', title: '新记', body: '内容', modified: 2000, attachments: false }];
  };
  const values = [];
  for await (const value of appleNotes(['selected-account'], report, { records, read, now: 6000 })) values.push(value);
  assert.deepEqual(calls, ['metadata', 'selected']);
  assert.equal(values.length, 1); assert.equal(values[0].key, 'new');
  assert.equal(report.unchanged, 1); assert.equal(report.skipped, 1);
  calls.length = 0;
  for await (const value of appleNotes(['selected-account'], report, { records: { ...records, [appleKey('selected-account', 'new')]: { modified: 2000, savedAt: 6000 } }, read })) {
    assert.fail('不应重复读取正文');
  }
  assert.deepEqual(calls, ['metadata']);
});

test('Apple 元数据读取和指定读取均不暴露未选择的正文', () => {
  const stub = id => ({ id: () => id, container: () => ({ name: () => 'Notes', shared: () => false }),
    passwordProtected: () => false, shared: () => false, modificationDate: () => new Date(1000),
    name: () => '标题', plaintext: () => { assert.equal(id, 'selected'); return '正文'; }, attachments: () => [] });
  const app = { accounts: { byId: () => ({ notes: () => [stub('other'), stub('selected')] }) } };
  assert.equal(snapshot(['metadata', 'account'], app).length, 2);
  assert.equal(snapshot(['selected', 'account', '["selected"]'], app)[0].body, '正文');
});

test('监听不要求模型；缺少登录或账号时不读取备忘录，后台参数不含秘密', () => fixture(async (root, config) => {
  assert.equal((await watchReadiness(config)).configured, true);
  const missing = { ...config, appleAccounts: [], profile: join(root, 'missing') };
  assert.deepEqual((await watchReadiness(missing)).missing, ['appleAccounts', 'login']);
  await assert.rejects(watch(missing, () => { assert.fail('不应连接'); }), /尚未就绪/);
  assert.deepEqual(watchOptions({}), { intervalMs: 3000, settleMs: 2000 });
  for (const value of [0, -1, 999, 60_001, NaN, '3000']) assert.throws(() => watchOptions({ watch: { intervalMs: value } }));
  const properties = launchProperties(join(root, 'config with spaces.json'), config, '/node path/node');
  assert.equal(properties.KeepAlive, true); assert.equal(properties.Umask, 0o077);
  assert.equal(properties.ProgramArguments[0], '/node path/node');
  assert.equal(properties.ProgramArguments.at(-1), join(root, 'config with spaces.json'));
  assert(!JSON.stringify(properties).includes(profile.cookie));
}));

test('同一输出目录只允许一个监听进程，正常结束回收锁与会话', () => fixture(async (root, config) => {
  const client = server(); let release, started;
  const ready = new Promise(resolve => { started = resolve; });
  const blocked = new Promise(resolve => { release = resolve; });
  const first = watch(config, async () => client, { once: true, report: () => {}, collect: async function* () { started(); await blocked; } });
  await ready;
  await assert.rejects(watch(config, async () => client, { once: true }), /已有备忘录监听/);
  release(); await first;
  await assert.rejects(stat(join(config.output, '.apple-watch.lock')), { code: 'ENOENT' });
}));

test('新 Mac 可直接初始化 Apple-only 配置，不要求文件目录且不覆盖旧配置', () => fixture(async root => {
  const main = fileURLToPath(new URL('../src/main.mjs', import.meta.url));
  const path = join(root, 'apple-only.json');
  const args = [main, 'notes', 'init', '--config', path, '--output', join(root, 'apple-output')];
  await execute(process.execPath, args);
  const created = await readFile(path, 'utf8');
  assert.deepEqual(JSON.parse(created).roots, []);
  assert.deepEqual(JSON.parse(created).appleAccounts, []);
  await assert.rejects(execute(process.execPath, args), /配置已存在/);
  assert.equal(await readFile(path, 'utf8'), created);
}));
