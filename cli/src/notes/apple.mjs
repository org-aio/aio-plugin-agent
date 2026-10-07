import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { digest } from '../session/storage.mjs';

const execute = promisify(execFile);

// 通过 Notes 的纯文本投影读取，HTML 标签不能切断字段名与秘密值。
export function snapshot(args, app) {
  if (args[0] === 'accounts') return app.accounts().map(function(a) { return { id: a.id(), name: a.name() }; });
  var account = app.accounts.byId(args[1]);
  var selected = args[0] === 'selected' ? JSON.parse(args[2]) : null;
  var result = [];
  account.notes().forEach(function(n) {
    var id = n.id();
    if (selected && selected.indexOf(id) < 0) return;
    var folder = n.container();
    if (n.passwordProtected() || n.shared() || folder.shared() || /^(recently deleted|最近删除|最近刪除)$/i.test(folder.name())) {
      result.push({ skipped: true }); return;
    }
    var modified = n.modificationDate().getTime();
    if (args[0] === 'metadata') { result.push({ id: id, modified: modified }); return; }
    var body = n.plaintext();
    if (n.modificationDate().getTime() !== modified) { result.push({ skipped: true }); return; }
    result.push({ id: id, title: n.name(), body: body, modified: modified, attachments: n.attachments().length > 0 });
  });
  return result;
}

async function script(operation, accountId, selected) {
  if (process.platform !== 'darwin') throw new Error('Apple 备忘录采集只支持 macOS');
  const code = `function run(args) { return JSON.stringify((${snapshot.toString()})(args, Application('Notes'))); }`;
  try {
    const { stdout } = await execute('/usr/bin/osascript', ['-l', 'JavaScript', '-e', code, operation, accountId || '', JSON.stringify(selected || [])], { maxBuffer: 32 * 1024 * 1024, timeout: 60_000 });
    return JSON.parse(stdout);
  } catch { throw new Error('无法读取 Apple 备忘录；检查 macOS 自动化权限与账号选择，原笔记保留'); }
}

export const appleAccounts = () => script('accounts');
export const appleKey = (account, key) => digest(JSON.stringify([account, key]));

export async function* appleNotes(accounts, report, options = {}) {
  const read = options.read || script;
  const now = options.now ?? Date.now();
  const candidates = [];
  if (options.records) {
    for (const account of accounts) {
      for (const value of await read('metadata', account)) {
        if (value.skipped) { report.skipped++; continue; }
        const record = options.records[appleKey(account, value.id)];
        if (record?.modified === value.modified && (record.savedAt || record.blocked || record.retryAt > now)) {
          report.unchanged++; continue;
        }
        candidates.push({ ...value, account, attempt: record?.attempt || 0 });
      }
    }
    candidates.sort((a, b) => a.attempt - b.attempt || b.modified - a.modified || a.id.localeCompare(b.id));
  }
  const selected = candidates.slice(0, options.maxNotes || 20);
  for (const account of accounts) {
    const ids = selected.filter(value => value.account === account).map(value => value.id);
    if (options.records && !ids.length) continue;
    for (const value of await read(options.records ? 'selected' : 'notes', account, ids)) {
      if (value.skipped) { report.skipped++; continue; }
      yield { kind: 'apple', account, key: value.id, title: value.title, text: value.body,
        digest: digest(value.body), identity: digest(JSON.stringify([value.title, digest(value.body)])),
        modified: value.modified, attachments: value.attachments };
    }
  }
}
