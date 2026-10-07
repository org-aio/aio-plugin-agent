import { mkdir, open, rm } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { digest, privateJson, privateWrite } from '../session/storage.mjs';
import { id } from '../memory/wiki.mjs';
import { noteText } from './files.mjs';
import { appleNotes, appleKey } from './apple.mjs';

export function watchOptions(config) {
  const intervalMs = config.watch?.intervalMs ?? 3000;
  const settleMs = config.watch?.settleMs ?? 2000;
  if (!Number.isInteger(intervalMs) || intervalMs < 1000 || intervalMs > 60_000 ||
      !Number.isInteger(settleMs) || settleMs < 1000 || settleMs > 60_000) {
    throw new Error('监听间隔及稳定等待必须是 1000–60000 毫秒');
  }
  return { intervalMs, settleMs };
}

export async function watchReadiness(config) {
  const missing = [];
  try { id(config.spaceId); } catch { missing.push('spaceId'); }
  if (!config.appleAccounts?.length) missing.push('appleAccounts');
  try {
    const profile = await privateJson(config.profile);
    if (profile.version !== 1 || !profile.cookie || !profile.origin || !profile.userId || !profile.tenantId) missing.push('login');
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    missing.push('login');
  }
  return { configured: !missing.length, missing, ...watchOptions(config) };
}

export async function watchState(config, client) {
  const scope = digest(JSON.stringify([client.profile.origin, client.profile.userId, client.profile.tenantId, config.spaceId]));
  const path = join(resolve(config.output), 'apple-watch.json');
  let state;
  try { state = await privateJson(path); }
  catch (error) {
    if (error.code !== 'ENOENT') throw error;
    state = { version: 1, scope, records: {} };
  }
  if (state.version !== 1 || state.scope !== scope || !state.records || typeof state.records !== 'object' || Array.isArray(state.records)) {
    throw new Error('监听状态属于其他用户或空间，请选择独立输出目录');
  }
  return { state, path };
}

export async function captureChanges(config, client, state, path, options = {}) {
  const now = options.now ?? Date.now();
  const { settleMs } = watchOptions(config);
  const report = { captured: 0, pending: 0, unchanged: 0, skipped: 0, oversized: 0, withAttachments: 0, failed: 0 };
  const collect = options.collect || appleNotes;
  // 先持久保存随机请求 ID；提交后丢失响应或进程崩溃仍可原样重试。
  for await (const note of collect(config.appleAccounts, report, { records: state.records, maxNotes: config.batchSize || 20, now })) {
    if (options.signal?.aborted) break;
    const key = appleKey(note.account, note.key);
    if (!note.text.trim()) { report.skipped++; continue; }
    const text = noteText(note).replace(/\r\n/g, '\n').replace(/^[ \t\r\n]+|[ \t\r\n]+$/g, '');
    const identity = digest(text);
    const previous = state.records[key];
    if (previous?.identity === identity && previous.savedAt) {
      previous.modified = note.modified;
      report.unchanged++;
      await privateWrite(path, JSON.stringify(state) + '\n');
      continue;
    }
    const record = previous?.identity === identity ? previous : {
      identity, requestId: randomUUID(), observedAt: now, attempt: 0,
    };
    record.modified = note.modified;
    state.records[key] = record;
    await privateWrite(path, JSON.stringify(state) + '\n');
    if (record.blocked || record.retryAt > now) { report.skipped++; continue; }
    if (Buffer.byteLength(text) > 100_000) {
      record.blocked = true;
      report.oversized++;
      await privateWrite(path, JSON.stringify(state) + '\n');
      continue;
    }
    if (now - record.observedAt < settleMs) {
      report.pending++; continue;
    }
    if (note.attachments) report.withAttachments++;
    record.attempt++;
    try {
      const source = await client.memory('POST', '/capture', {
        requestId: record.requestId, text, spaceId: config.spaceId, origin: 'import',
        reference: `apple-note:${key}`, deduplicate: true,
      });
      if (source.spaceId !== config.spaceId) throw new Error('收件空间不一致');
      record.sourceId = id(source.id);
      record.savedAt = now;
      delete record.retryAt;
      report.captured++;
    } catch {
      record.retryAt = now + Math.min(60_000, 3000 * 2 ** Math.min(record.attempt - 1, 5));
      report.failed++;
    }
    await privateWrite(path, JSON.stringify(state) + '\n');
    // 网络中断时不连续等待整批请求，未收件笔记在下一轮继续。
    if (report.failed) break;
  }
  return report;
}

export async function watch(config, loadClient, options = {}) {
  const ready = await watchReadiness(config);
  if (!ready.configured) throw new Error(`监听尚未就绪：${ready.missing.join(', ')}`);
  const output = resolve(config.output);
  await mkdir(output, { recursive: true, mode: 0o700 });
  const lockPath = join(output, '.apple-watch.lock');
  const lock = await acquireLock(lockPath);
  const reportPath = join(output, 'last-watch.json');
  const report = options.report || (value => console.log(JSON.stringify(value)));
  let failures = 0;
  let validated = false;
  try {
    while (!options.signal?.aborted) {
      let client;
      try {
        // 每轮重新读取会话，重新登录后无需重启；不复用旧用户授权。
        client = await loadClient();
        const { state, path } = await watchState(config, client);
        if (!validated) {
          const spaces = await client.memory('GET', '/spaces');
          if (!spaces.some(space => space.id === config.spaceId && ['OWNER', 'EDITOR'].includes(space.role))) {
            throw new Error('所选空间不存在或没有写入权限');
          }
        }
        const counts = await captureChanges(config, client, state, path, options);
        const result = { at: new Date().toISOString(), pid: process.pid, ...counts };
        await privateWrite(reportPath, JSON.stringify(result) + '\n');
        if (!validated || counts.captured || counts.failed || counts.oversized) report(result);
        validated = true;
        failures = counts.failed ? failures + 1 : 0;
      } catch (error) {
        failures++;
        const result = { at: new Date().toISOString(), pid: process.pid, failed: 1, error: error.message };
        await privateWrite(reportPath, JSON.stringify(result) + '\n');
        report(result);
      } finally { if (client) await client.close(); }
      if (options.once) break;
      const wait = failures ? Math.min(60_000, ready.intervalMs * 2 ** Math.min(failures, 5)) : ready.intervalMs;
      try { await delay(wait, undefined, { signal: options.signal }); }
      catch (error) { if (error.name !== 'AbortError') throw error; }
    }
  } finally { await lock.close(); await rm(lockPath, { force: true }); }
}

async function acquireLock(path) {
  let lock;
  try { lock = await open(path, 'wx', 0o600); }
  catch (error) {
    if (error.code !== 'EEXIST') throw error;
    const prior = await privateJson(path);
    if (!Number.isSafeInteger(prior.pid) || prior.pid < 1) throw new Error('监听锁无效');
    try { process.kill(prior.pid, 0); throw new Error('已有备忘录监听进程运行'); }
    catch (error) { if (error.code !== 'ESRCH') throw error; }
    await rm(path);
    lock = await open(path, 'wx', 0o600);
  }
  await lock.writeFile(JSON.stringify({ pid: process.pid }));
  return lock;
}
