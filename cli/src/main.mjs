#!/usr/bin/env node
import { parseArgs } from 'node:util';
import { readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { homedir } from 'node:os';
import { randomUUID } from 'node:crypto';
import { HostClient, login } from './session/client.mjs';
import { privateWrite } from './session/storage.mjs';
import { id, scoped, exportSource } from './memory/wiki.mjs';
import { configuration, readiness, scan, run } from './notes/pipeline.mjs';
import { appleAccounts } from './notes/apple.mjs';

const HELP = `AIO 智能体 CLI
  aio-agent login --url https://aio.addzero.site --account 用户 --password-stdin
  aio-agent memory spaces
  aio-agent memory models
  aio-agent memory search "关键词" --space 空间ID
  aio-agent memory get 节点ID --space 空间ID
  aio-agent memory capture --file 笔记.md --space 空间ID [--request-id 稳定ID]
  aio-agent memory source 来源ID --space 空间ID
  aio-agent memory export 来源ID --space 空间ID --output 目录
  aio-agent notes init --config 绝对路径 --root 笔记目录 --output wiki目录
  aio-agent notes accounts
  aio-agent notes status --config 配置文件
  aio-agent notes scan --config 配置文件
  aio-agent notes run --config 配置文件

通用：--profile 会话文件，默认 ~/.config/aio-agent/session.json
capture 不输出正文。检索、get、source 仅返回服务端净化内容。`;

async function stdin() {
  const parts = []; let length = 0;
  for await (const part of process.stdin) {
    length += part.length;
    if (length > 100_000) throw new Error('标准输入超过配额');
    parts.push(part);
  }
  return Buffer.concat(parts).toString('utf8');
}

async function main() {
  const { values: args, positionals } = parseArgs({ allowPositionals: true, options: {
    help: { type: 'boolean', short: 'h' }, profile: { type: 'string' }, url: { type: 'string' },
    account: { type: 'string' }, 'password-stdin': { type: 'boolean' }, space: { type: 'string' },
    file: { type: 'string' }, output: { type: 'string' }, config: { type: 'string' },
    root: { type: 'string', multiple: true }, 'request-id': { type: 'string' },
  } });
  if (args.help || !positionals.length) return console.log(HELP);
  const profile = resolve(args.profile || join(homedir(), '.config', 'aio-agent', 'session.json'));
  const [command, action, value] = positionals;
  if (command === 'login') {
    if (!args.url || !args.account || !args['password-stdin']) throw new Error('login 需要 --url、--account、--password-stdin');
    return console.log(JSON.stringify(await login(profile, args.url, args.account, (await stdin()).replace(/\r?\n$/, ''))));
  }
  if (command === 'notes' && action === 'accounts') return console.log(JSON.stringify(await appleAccounts(), null, 2));
  if (command === 'notes' && action === 'init') {
    if (!args.config || !args.root?.length || !args.output) throw new Error('init 需要 --config、--root、--output');
    // 不覆盖已有任务配置，以免改变已授权清理范围或目标空间。
    try { await readFile(args.config); throw new Error('配置已存在，请直接编辑'); } catch (error) { if (error.code !== 'ENOENT') throw error; }
    await privateWrite(resolve(args.config), JSON.stringify({ version: 1, profile, roots: args.root.map(value => resolve(value)),
      appleAccounts: [], output: resolve(args.output), spaceId: null, modelBinding: null, model: null, cleanup: 'none', batchSize: 20 }, null, 2) + '\n');
    return console.log('已创建笔记配置；填入实际空间与模型绑定后启用整理。');
  }
  if (command === 'notes') {
    if (!args.config) throw new Error('需要 --config');
    const config = await configuration(resolve(args.config));
    if (action === 'status') return console.log(JSON.stringify(await readiness(config), null, 2));
    if (action === 'scan') return console.log(JSON.stringify(await scan(config), null, 2));
    if (action !== 'run') throw new Error('未知笔记命令');
    const client = await HostClient.load(config.profile);
    try { console.log(JSON.stringify(await run(config, client), null, 2)); } finally { await client.close(); }
    return;
  }
  if (command !== 'memory') throw new Error('未知命令，使用 --help 查看');
  const client = await HostClient.load(profile);
  try {
    let result;
    if (action === 'spaces') result = await client.memory('GET', '/spaces');
    else if (action === 'models') result = (await client.agent('GET', '/settings')).providers;
    else {
      const space = id(args.space);
      switch (action) {
        case 'search':
          if (!value?.trim() || value.length > 180) throw new Error('搜索关键词应为 1–180 字符');
          const graph = await client.memory('POST', scoped('/search', space), { query: value, limit: 8 });
          const context = await client.memory('POST', scoped('/context', space), { nodeIds: graph.nodes.map(node => node.id), depth: 1, maxCharacters: 6000 });
          result = { ...context, citations: graph.nodes.map(node => ({ id: node.id, title: node.title })) }; break;
        case 'get': result = await client.memory('GET', scoped(`/nodes/${id(value)}`, space)); break;
        case 'source': result = await client.memory('GET', scoped(`/sources/${id(value)}`, space)); break;
        case 'capture': {
          const text = args.file ? await readFile(resolve(args.file), 'utf8') : await stdin();
          if (!text.trim() || Buffer.byteLength(text) > 100_000) throw new Error('收件必须是 1–100000 字节的文本，批量长笔记请用 notes run');
          const source = await client.memory('POST', '/capture', { requestId: args['request-id'] || randomUUID(), text, spaceId: space, origin: 'import' });
          result = { id: source.id, spaceId: source.spaceId, status: source.status }; break;
        }
        case 'export':
          if (!args.output) throw new Error('需要 --output');
          result = await exportSource(client, id(value), space, resolve(args.output)); break;
        default: throw new Error('未知记忆命令');
      }
    }
    console.log(JSON.stringify(result, null, 2));
  } finally { await client.close(); }
}
main().catch(error => { console.error(error instanceof SyntaxError ? '配置或服务响应不是有效 JSON' : error.message); process.exitCode = 1; });
