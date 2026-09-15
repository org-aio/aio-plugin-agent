import { mkdir, rename, lstat } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { homedir } from 'node:os';
import { randomUUID } from 'node:crypto';
import { readNote } from './files.mjs';

export async function trashVerified(note, proofs, policy, canonical, trashRoot = join(homedir(), '.Trash', 'AIO 已整理')) {
  if (policy === 'none' || note.kind !== 'file' || note.attachments || !proofs.length ||
      proofs.some(proof => !proof.verified || proof.hasSecrets)) return null;
  if (policy === 'duplicates') {
    if (!canonical || canonical === note.key) return null;
    const kept = await readNote(canonical);
    if (kept.identity !== note.identity) return null;
  } else if (policy !== 'archived') throw new Error('未知清理策略');
  const current = await readNote(note.key);
  if (current.digest !== note.digest || current.inode !== note.inode || current.device !== note.device) return null;
  await mkdir(trashRoot, { recursive: true, mode: 0o700 });
  const target = join(trashRoot, randomUUID() + '-' + basename(note.key));
  // 只做可恢复移动；跨文件系统时失败并保留原件，不降级成复制后 unlink。
  await rename(note.key, target);
  const moved = await readNote(target);
  if (moved.digest !== note.digest || moved.inode !== note.inode) {
    try { await lstat(note.key); } catch (error) { if (error.code === 'ENOENT') await rename(target, note.key); }
    throw new Error('清理时原件发生变化，已保留可恢复副本');
  }
  return target;
}
