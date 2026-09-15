import { privateJson, privateWrite } from './storage.mjs';

const AGENT = 'https://github.com/zjarlin/aio-plugin-agent.git';

export function origin(value) {
  const url = new URL(value);
  if (url.username || url.password || url.search || url.hash || url.pathname !== '/' ||
      (url.protocol !== 'https:' && !(url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname))))
    throw new Error('宿主地址必须是 HTTPS 来源，本机测试可用 loopback HTTP');
  return url.origin;
}

async function responseJson(response) {
  if (!response.ok) throw new Error(response.status === 401 ? 'AIO 会话已过期，请重新登录' : `AIO 请求失败（HTTP ${response.status}）`);
  if (response.status === 204) return null;
  let size = 0; const parts = [];
  for await (const part of response.body) {
    size += part.length;
    if (size > 8 * 1024 * 1024) throw new Error('AIO 响应超过配额');
    parts.push(part);
  }
  return JSON.parse(Buffer.concat(parts).toString('utf8'));
}

export async function login(profilePath, url, account, password, fetcher = fetch) {
  const base = origin(url);
  const response = await fetcher(base + '/api/auth/login', {
    method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ account, password }), redirect: 'error', signal: AbortSignal.timeout(30_000),
  });
  const result = await responseJson(response);
  const cookie = response.headers.getSetCookie().map(value => value.split(';')[0]).find(value => value.startsWith('aio_session='));
  const session = result?.data;
  if (!cookie || !session?.user_id || !session?.tenant_id) throw new Error('登录响应缺少有效会话');
  await privateWrite(profilePath, JSON.stringify({ version: 1, origin: base, cookie, userId: session.user_id, tenantId: session.tenant_id }) + '\n');
  return { account, workspace: session.tenant_label };
}

export class HostClient {
  constructor(profile, fetcher = fetch) { this.profile = { ...profile, origin: origin(profile.origin) }; this.fetch = fetcher; }
  static async load(path) { return new HostClient(await privateJson(path)); }
  async request(path, method = 'GET', body) {
    if (!path.startsWith('/api/') || path.includes('..')) throw new Error('无效宿主路径');
    const response = await this.fetch(this.profile.origin + path, {
      method, headers: { cookie: this.profile.cookie, 'content-type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body), redirect: 'error', signal: AbortSignal.timeout(45_000),
    });
    return responseJson(response);
  }
  async connect() {
    const session = (await this.request('/api/auth/session')).data;
    if (session?.user_id !== this.profile.userId || session?.tenant_id !== this.profile.tenantId)
      throw new Error('登录用户或工作区已改变，请为目标工作区重新登录');
    const catalog = (await this.request('/api/runtime/catalog')).data;
    const plugin = catalog.plugins.find(value => value.git === AGENT && value.state === 'active');
    const page = plugin && catalog.pages.find(value => value.id === `component:${plugin.source_id}:chat`);
    if (!page) throw new Error('当前工作区尚未启用 AIO v2 智能体插件');
    const grant = (await this.request('/api/runtime/frontend/mount', 'POST', { page_id: page.id })).data;
    if (grant.abi !== 2 || !/^[a-zA-Z0-9_-]+$/.test(grant.token)) throw new Error('宿主未返回有效 v2 请求授权');
    this.grant = grant.token;
    this.renewedAt = Date.now();
  }
  async agent(method, path, body) {
    if (!this.grant) await this.connect();
    if (Date.now() - this.renewedAt > 30_000) {
      await this.request(`/api/runtime/components/${this.grant}/renew`, 'POST');
      this.renewedAt = Date.now();
    }
    const bytes = body === undefined ? [] : [...Buffer.from(JSON.stringify(body))];
    const result = (await this.request(`/api/runtime/components/${this.grant}/request`, 'POST', {
      method, path, query: null, headers: [{ name: 'content-type', value: 'application/json' }], body: bytes,
    })).data;
    if (result.status < 200 || result.status >= 300) throw new Error(`智能体请求失败（HTTP ${result.status}），资料已保留`);
    return result.body?.length ? JSON.parse(Buffer.from(result.body).toString('utf8')) : null;
  }
  memory(method, path, body = null) { return this.agent('POST', '/memory', { method, path, body }); }
  async close() {
    if (this.grant) {
      try { await this.request(`/api/runtime/frontend/${this.grant}`, 'DELETE'); } catch { /* 短租约仍会由宿主回收。 */ }
      this.grant = null;
    }
  }
}
