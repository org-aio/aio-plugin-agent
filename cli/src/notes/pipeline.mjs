import { mkdir, open, rm } from 'node:fs/promises';
import { join, resolve, isAbsolute } from 'node:path';
import { digest, privateJson, privateWrite } from '../session/storage.mjs';
import { fileNotes, chunks } from './files.mjs';
import { appleNotes } from './apple.mjs';
import { exportSource, id, writeIndex } from '../memory/wiki.mjs';
import { trashVerified } from './cleanup.mjs';

export async function configuration(path) {
  const config = await privateJson(path);
  if (config.version !== 1 || !Array.isArray(config.roots) || config.roots.some(value => !isAbsolute(value)) ||
      !isAbsolute(config.output || '') || !isAbsolute(config.profile || '') ||
      !['none', 'duplicates', 'archived'].includes(config.cleanup || '') ||
      (config.batchSize !== undefined && (!Number.isInteger(config.batchSize) || config.batchSize < 1 || config.batchSize > 100)) ||
      (config.appleAccounts && (!Array.isArray(config.appleAccounts) || config.appleAccounts.some(value => typeof value !== 'string'))))
    throw new Error('笔记配置无效，路径必须是绝对路径');
  return config;
}

export async function scan(config) {
  const report = { files: 0, apple: 0, skipped: 0, duplicates: 0, withAttachments: 0, oversized: 0 };
  const seen = new Set();
  for await (const note of collect(config, report)) {
    report[note.kind === 'file' ? 'files' : 'apple']++;
    if (note.attachments) report.withAttachments++;
    try { chunks(note); } catch { report.oversized++; }
    if (seen.has(note.identity)) report.duplicates++;
    seen.add(note.identity);
  }
  return report;
}

export async function readiness(config) {
  const missing = [];
  try { id(config.spaceId); } catch { missing.push('spaceId'); }
  if (!config.modelBinding) missing.push('modelBinding');
  if (!config.model) missing.push('model');
  try {
    const profile = await privateJson(config.profile);
    if (profile.version !== 1 || !profile.cookie || !profile.origin || !profile.userId || !profile.tenantId) missing.push('login');
  } catch (error) { if (error.code !== 'ENOENT') throw error; missing.push('login'); }
  return { configured: !missing.length, missing, cleanup: config.cleanup, fileRoots: config.roots.length, appleAccounts: (config.appleAccounts || []).length };
}

async function* collect(config, report) {
  yield* fileNotes(config.roots, resolve(config.output), report);
  yield* appleNotes(config.appleAccounts || [], report);
}

async function binding(client, config) {
  id(config.spaceId);
  if (!config.modelBinding || !config.model) throw new Error('尚未指定 q3-4b 的实际模型配置；不会使用其他模型代替');
  const settings = await client.agent('GET', '/settings');
  const provider = settings.providers.find(value => value.id === config.modelBinding);
  if (!provider || provider.model !== config.model) throw new Error('整理模型与指定模型不一致');
  const spaces = await client.memory('GET', '/spaces');
  const space = spaces.find(value => value.id === config.spaceId);
  if (!space || space.role === 'READER' || space.modelBinding !== config.modelBinding) throw new Error('整理空间没有绑定指定模型或没有写入权限');
}

export async function run(config, client, options = {}) {
  await client.connect();
  await binding(client, config);
  const output = resolve(config.output);
  await mkdir(output, { recursive: true, mode: 0o700 });
  const lockPath = join(output, '.run.lock');
  let lock;
  try { lock = await open(lockPath, 'wx', 0o600); }
  catch (error) {
    if (error.code !== 'EEXIST') throw error;
    const prior = await privateJson(lockPath);
    if (!Number.isSafeInteger(prior.pid) || prior.pid < 1) throw new Error('整理锁无效，请检查 .run.lock');
    try { process.kill(prior.pid, 0); throw new Error('已有整理任务运行'); }
    catch (error) { if (error.code !== 'ESRCH') throw error; }
    await rm(lockPath);
    lock = await open(lockPath, 'wx', 0o600);
  }
  await lock.writeFile(JSON.stringify({ pid: process.pid, startedAt: new Date().toISOString() }));
  const statePath = join(output, 'state.json');
  const scope = digest(JSON.stringify([client.profile.origin, client.profile.userId, client.profile.tenantId, config.spaceId]));
  const report = { captured: 0, verified: 0, pending: 0, trashed: 0, skipped: 0, failed: 0 };
  const canonical = new Map();
  try {
    let state;
    try { state = await privateJson(statePath); }
    catch (error) { if (error.code !== 'ENOENT') throw error; state = { version: 1, scope, records: {} }; }
    if (state.version !== 1 || state.scope !== scope) throw new Error('输出目录属于其他用户或空间，请选择独立目录');
    const notes = [];
    for await (const note of collect(config, report)) {
      notes.push(note);
      if (note.kind === 'file' && !canonical.has(note.identity)) canonical.set(note.identity, note.key);
    }
    // 新资料先收件，避免持续失败的旧任务占满每次批次。
    const keyFor = note => digest(JSON.stringify([scope, note.key]));
    notes.sort((a, b) => Number(Boolean(state.records[keyFor(a)])) - Number(Boolean(state.records[keyFor(b)])));
    let batch = 0;
    for (const note of notes) {
      const key = keyFor(note);
      const previous = state.records[key];
      if (previous?.identity === note.identity && previous.verifiedAt && previous.cleanup === config.cleanup) continue;
      if (Date.now() - note.modified < 15 * 60_000) { report.skipped++; continue; }
      if (batch >= (config.batchSize || 20)) { report.skipped++; continue; }
      const record = previous?.identity === note.identity ? previous : { identity: note.identity, path: note.key, digest: note.digest, sources: [] };
      // 已验证记录也重新读取来源和版本；删除或撤权后不能沿用旧的清理许可。
      try {
        const parts = chunks(note);
        for (let index = record.sources.length; index < parts.length; index++) {
          const source = await client.memory('POST', '/capture', {
            requestId: `local-note-v1:${note.identity}:${index}`, text: parts[index], spaceId: config.spaceId,
            origin: 'import', reference: `local-note:${digest(note.key)}:${index + 1}/${parts.length}`,
          });
          if (source.spaceId !== config.spaceId) throw new Error('收件空间不一致');
          record.sources.push(id(source.id));
          state.records[key] = record;
          await privateWrite(statePath, JSON.stringify(state, null, 2) + '\n');
          report.captured++;
        }
        const proofs = [];
        for (const sourceId of record.sources) proofs.push(await exportSource(client, sourceId, config.spaceId, output));
        if (proofs.every(proof => proof.verified)) {
          const target = await trashVerified(note, proofs, config.cleanup, canonical.get(note.identity), options.trashRoot);
          record.verifiedAt = new Date().toISOString();
          record.proofs = proofs;
          record.cleanup = config.cleanup;
          report.verified++;
          if (target) { record.trash = target; report.trashed++; }
        } else { record.proofs = []; report.pending++; }
        state.records[key] = record;
        batch++;
        await privateWrite(statePath, JSON.stringify(state, null, 2) + '\n');
      } catch {
        delete record.verifiedAt;
        record.proofs = [];
        record.cleanup = null;
        state.records[key] = record;
        await privateWrite(statePath, JSON.stringify(state, null, 2) + '\n');
        report.failed++; batch++;
      }
    }
    await writeIndex(output);
    await privateWrite(join(output, 'last-run.json'), JSON.stringify({ at: new Date().toISOString(), ...report }, null, 2) + '\n');
    return report;
  } finally { await lock.close(); await rm(lockPath, { force: true }); }
}
