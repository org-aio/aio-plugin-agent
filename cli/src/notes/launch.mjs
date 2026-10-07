import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { mkdir, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { privateWrite } from '../session/storage.mjs';
import { watchReadiness } from './watch.mjs';

const execute = promisify(execFile);
const label = 'site.addzero.aio.agent.notes';

export function launchProperties(configPath, config, node = process.execPath) {
  const output = resolve(config.output);
  return {
    Label: label,
    ProgramArguments: [node, fileURLToPath(new URL('../main.mjs', import.meta.url)), 'notes', 'watch', '--config', resolve(configPath)],
    RunAtLoad: true, KeepAlive: true, ThrottleInterval: 30, ProcessType: 'Background', Umask: 0o077,
    StandardOutPath: join(output, 'apple-watch.log'), StandardErrorPath: join(output, 'apple-watch-error.log'),
  };
}

async function stop(service) {
  try { await execute('/bin/launchctl', ['print', service], { timeout: 15_000 }); }
  catch (error) {
    if (error.code === 113) return;
    throw error;
  }
  await execute('/bin/launchctl', ['bootout', service], { timeout: 15_000 });
}

export async function installWatch(path, config) {
  if (process.platform !== 'darwin') throw new Error('后台备忘录监听安装只支持 macOS');
  const ready = await watchReadiness(config);
  if (!ready.configured) throw new Error(`监听尚未就绪：${ready.missing.join(', ')}`);
  const properties = launchProperties(path, config);
  const xml = await new Promise((resolve, reject) => {
    const child = execFile('/usr/bin/plutil', ['-convert', 'xml1', '-o', '-', '-'], { timeout: 15_000 }, (error, stdout) => {
      if (error) reject(error); else resolve(stdout);
    });
    child.stdin.end(JSON.stringify(properties));
  });
  await mkdir(resolve(config.output), { recursive: true, mode: 0o700 });
  const plist = join(homedir(), 'Library', 'LaunchAgents', label + '.plist');
  await privateWrite(plist, xml);
  const domain = `gui/${process.getuid()}`;
  await stop(`${domain}/${label}`);
  await execute('/bin/launchctl', ['bootstrap', domain, plist], { timeout: 15_000 });
  return { installed: true, label, ...ready };
}

export async function removeWatch() {
  if (process.platform !== 'darwin') throw new Error('后台备忘录监听卸载只支持 macOS');
  await stop(`gui/${process.getuid()}/${label}`);
  await rm(join(homedir(), 'Library', 'LaunchAgents', label + '.plist'), { force: true });
  return { installed: false, label, originalsPreserved: true };
}
