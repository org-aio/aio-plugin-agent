import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'node:http';
import { HostClient, login, origin } from '../src/session/client.mjs';

test('正式登录和 v2 桥只传会话，不接受客户端指定用户', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'aio-agent-session-'));
  let tenant = 'workspace', calls = 0;
  const session = () => ({ user_id: 'user', tenant_id: tenant, tenant_label: '工作区' });
  const server = createServer(async (req, res) => {
    const send = value => res.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ data: value }));
    const parts = []; for await (const part of req) parts.push(part);
    const body = parts.length ? JSON.parse(Buffer.concat(parts)) : undefined;
    if (req.url === '/api/auth/login') {
      assert.equal(body.password, 'test-password');
      res.setHeader('set-cookie', 'aio_session=test-session; HttpOnly; SameSite=Lax'); return send(session());
    }
    assert.equal(req.headers.cookie, 'aio_session=test-session');
    assert.equal(req.headers['x-aio-user-id'], undefined);
    if (req.url === '/api/auth/session') return send(session());
    if (req.url === '/api/runtime/catalog') return send({ plugins: [{ git: 'https://github.com/zjarlin/aio-plugin-agent.git', source_id: 'source', state: 'active' }], pages: [{ id: 'component:source:chat' }] });
    if (req.url === '/api/runtime/frontend/mount') { assert.equal(body.page_id, 'component:source:chat'); return send({ abi: 2, token: 'grant' }); }
    if (req.url === '/api/runtime/components/grant/request') {
      calls++; assert.equal(body.path, '/memory'); assert.equal(body.method, 'POST');
      assert.equal(JSON.parse(Buffer.from(body.body)).path, '/spaces');
      return send({ status: 200, headers: [], body: [...Buffer.from('[{"title":"个人空间"}]')] });
    }
    if (req.url === '/api/runtime/frontend/grant') return res.writeHead(204).end();
    res.writeHead(404).end();
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    const profile = join(directory, 'session.json');
    await login(profile, `http://127.0.0.1:${server.address().port}`, 'user', 'test-password');
    assert.equal((await stat(profile)).mode & 0o777, 0o600);
    const client = await HostClient.load(profile);
    assert.equal((await client.memory('GET', '/spaces'))[0].title, '个人空间');
    assert.equal(calls, 1);
    await client.close();
    tenant = 'other';
    await assert.rejects(client.connect(), /工作区已改变/);
    assert.equal(calls, 1);
  } finally { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); await rm(directory, { recursive: true }); }
});

test('会话不能被发送给带凭据、路径或非加密外部宿主', () => {
  for (const url of ['http://example.com', 'https://user:password@example.com', 'https://example.com/other', 'https://example.com?token=secret'])
    assert.throws(() => origin(url));
});
