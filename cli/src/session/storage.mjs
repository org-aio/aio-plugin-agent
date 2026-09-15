import { mkdir, open, rename, readFile, lstat, rm } from 'node:fs/promises';
import { dirname } from 'node:path';
import { randomUUID, createHash } from 'node:crypto';

export const digest = value => createHash('sha256').update(value).digest('hex');

export async function privateWrite(path, value) {
  await mkdir(dirname(path), { recursive: true, mode: 0o700 });
  const temporary = path + '.' + randomUUID() + '.tmp';
  const file = await open(temporary, 'wx', 0o600);
  try { await file.writeFile(value); await file.sync(); }
  finally { await file.close(); }
  try { await rename(temporary, path); }
  finally { await rm(temporary, { force: true }); }
}

export async function privateJson(path) {
  const info = await lstat(path);
  if (!info.isFile() || (process.platform !== 'win32' && (info.mode & 0o077)))
    throw new Error('配置文件必须是仅当前用户可读写的普通文件（0600）');
  return JSON.parse(await readFile(path, 'utf8'));
}
