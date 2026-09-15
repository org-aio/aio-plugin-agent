import { readdir, lstat, open, realpath } from 'node:fs/promises';
import { constants } from 'node:fs';
import { extname, basename, join, relative, isAbsolute } from 'node:path';
import { digest } from '../session/storage.mjs';

const ignored = new Set(['.git', '.obsidian', '.code-notes', '.Trash', 'node_modules', 'target', 'build', '.DS_Store']);
const formats = new Set(['.md', '.markdown', '.txt']);
export const within = (root, path) => { const part = relative(root, path); return part !== '..' && !part.startsWith('../') && !isAbsolute(part); };

export async function readNote(path) {
  const link = await lstat(path);
  if (!link.isFile() || link.isSymbolicLink()) throw new Error('不是普通笔记文件');
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const info = await file.stat();
    if (info.size > 8 * 1024 * 1024) throw new Error('笔记超过单文件配额');
    const bytes = await file.readFile();
    const after = await file.stat();
    if (info.mtimeMs !== after.mtimeMs || info.size !== after.size || link.ino !== info.ino) throw new Error('笔记正在修改');
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    if (text.includes('\0')) throw new Error('笔记不是 UTF-8 文本');
    const title = basename(path);
    return { kind: 'file', key: path, title, text, digest: digest(bytes),
      identity: digest(JSON.stringify([title, digest(bytes)])), modified: after.mtimeMs, inode: info.ino, device: info.dev,
      attachments: /!\[|<img\b|<audio\b|<video\b|<object\b|\]\((?!https?:|#|mailto:)[^)]+\)/i.test(text) };
  } finally { await file.close(); }
}

export async function* fileNotes(roots, output, report) {
  const seen = new Set();
  for (const configured of roots) {
    const rootInfo = await lstat(configured);
    if (rootInfo.isSymbolicLink() || !rootInfo.isDirectory()) throw new Error('笔记根目录必须是普通目录');
    const root = await realpath(configured);
    if (within(root, output) || within(output, root)) throw new Error('Wiki 输出目录必须与笔记根目录分离');
    async function* walk(path) {
      for (const entry of await readdir(path, { withFileTypes: true })) {
        if (entry.isSymbolicLink() || ignored.has(entry.name) || entry.name.startsWith('.')) continue;
        const full = join(path, entry.name);
        if (entry.isDirectory()) { yield* walk(full); continue; }
        if (!entry.isFile() || !formats.has(extname(entry.name).toLowerCase()) || seen.has(full)) continue;
        seen.add(full);
        try { yield await readNote(full); } catch { report.skipped++; }
      }
    }
    yield* walk(root);
  }
}

// 原文必须整篇进入 Memory 隔离器，不能先切断密码标签、JSON 或多行密钥。
// 超过统一收件上限时保留原件，等待支持大文件的服务端隔离入口。
export function chunks(note) {
  // JSON 原文不能加 Markdown 标题，否则服务端会失去结构化字段识别。
  const text = /^\s*[\[{]/.test(note.text) ? note.text : `# ${note.title}\n\n${note.text}`;
  if (Buffer.byteLength(text) > 100_000) throw new Error('笔记超过统一收件配额，未拆分原文并保留原件');
  return [text];
}
