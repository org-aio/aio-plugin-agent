import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { digest } from '../session/storage.mjs';

const execute = promisify(execFile);

// 通过 Notes 的纯文本投影读取，HTML 标签不能切断字段名与秘密值。
export function snapshot(args, app) {
  if (args[0] === 'accounts') return app.accounts().map(function(a) { return { id: a.id(), name: a.name() }; });
  var account = app.accounts.byId(args[1]);
  var result = [];
  account.notes().forEach(function(n) {
    var folder = n.container();
    if (n.passwordProtected() || n.shared() || folder.shared() || /^(recently deleted|最近删除|最近刪除)$/i.test(folder.name())) {
      result.push({ skipped: true }); return;
    }
    var modified = n.modificationDate().getTime();
    var body = n.plaintext();
    if (n.modificationDate().getTime() !== modified) { result.push({ skipped: true }); return; }
    result.push({ id: n.id(), title: n.name(), body: body, modified: modified, attachments: n.attachments().length > 0 });
  });
  return result;
}

async function script(operation, accountId) {
  if (process.platform !== 'darwin') throw new Error('Apple 备忘录采集只支持 macOS');
  const code = `function run(args) { return JSON.stringify((${snapshot.toString()})(args, Application('Notes'))); }`;
  try {
    const { stdout } = await execute('/usr/bin/osascript', ['-l', 'JavaScript', '-e', code, operation, accountId || ''], { maxBuffer: 32 * 1024 * 1024, timeout: 60_000 });
    return JSON.parse(stdout);
  } catch { throw new Error('无法读取 Apple 备忘录；检查 macOS 自动化权限与账号选择，原笔记保留'); }
}

export const appleAccounts = () => script('accounts');
export async function* appleNotes(accounts, report) {
  for (const account of accounts) {
    for (const value of await script('notes', account)) {
      if (value.skipped) { report.skipped++; continue; }
      yield { kind: 'apple', key: value.id, title: value.title, text: value.body,
        digest: digest(value.body), identity: digest(JSON.stringify([value.title, digest(value.body)])),
        modified: value.modified, attachments: value.attachments };
    }
  }
}
