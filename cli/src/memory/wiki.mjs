import { join } from 'node:path';
import { readFile, readdir } from 'node:fs/promises';
import { digest, privateWrite } from '../session/storage.mjs';

export function id(value) {
  if (!/^[a-f0-9]{32}$/.test(value || '')) throw new Error('记忆 ID 无效');
  return value;
}
export const scoped = (path, space) => `${path}?spaceId=${id(space)}`;

export async function exportSource(client, sourceId, spaceId, output) {
  const source = await client.memory('GET', scoped(`/sources/${id(sourceId)}`, spaceId));
  if (source.spaceId !== spaceId || source.status !== 'complete') return { verified: false, status: source.status };
  const edges = await client.memory('GET', scoped(`/nodes/${sourceId}/edges`, spaceId));
  const targets = [...new Set(edges.filter(edge => edge.source === sourceId && edge.relation === '来源').map(edge => id(edge.target)))];
  if (!targets.length) return { verified: false, status: 'empty_wiki' };
  const nodes = [];
  for (const nodeId of targets) {
    const node = await client.memory('GET', scoped(`/nodes/${nodeId}`, spaceId));
    const evidence = await client.memory('GET', scoped(`/nodes/${nodeId}/sources`, spaceId));
    if (!evidence.some(value => value.id === sourceId) || !Number.isInteger(node.version)) throw new Error('Wiki 来源依据或版本核对失败');
    nodes.push(node);
  }
  const related = new Map(edges.filter(edge => [sourceId, ...targets].includes(edge.source) && [sourceId, ...targets].includes(edge.target)).map(edge => [edge.id, edge]));
  for (const node of nodes) {
    for (const edge of await client.memory('GET', scoped(`/nodes/${node.id}/edges`, spaceId))) {
      if ([sourceId, ...targets].includes(edge.source) && [sourceId, ...targets].includes(edge.target)) related.set(edge.id, edge);
    }
  }
  const record = { sourceId, spaceId, sourceDigest: digest(source.text), nodes: nodes.map(node => ({ id: node.id, title: node.title, version: node.version })), edges: [...related.values()] };
  const sourcePath = join(output, 'sources', sourceId + '.md');
  await privateWrite(sourcePath, source.text);
  if (digest(await readFile(sourcePath)) !== record.sourceDigest) throw new Error('净化来源副本校验失败');
  for (const node of nodes) {
    const markdown = `# ${node.title}\n\n${node.content}\n\n---\nAIO 节点：${node.id} · 版本 ${node.version}\n来源：[${sourceId}](../../sources/${sourceId}.md)\n`;
    // 每个来源与节点版本分别留存，避免新来源覆盖旧依据。
    const path = join(output, 'entries', sourceId, node.id + '.md');
    await privateWrite(path, markdown);
    if (digest(await readFile(path)) !== digest(markdown)) throw new Error('Wiki 文件校验失败');
  }
  const fresh = await client.memory('GET', scoped(`/sources/${sourceId}`, spaceId));
  if (fresh.status !== 'complete' || digest(fresh.text) !== record.sourceDigest) throw new Error('导出过程中来源发生变化');
  for (const node of nodes) {
    const freshNode = await client.memory('GET', scoped(`/nodes/${node.id}`, spaceId));
    if (freshNode.version !== node.version) throw new Error('导出过程中 wiki 版本发生变化');
  }
  await privateWrite(join(output, 'manifests', sourceId + '.json'), JSON.stringify(record, null, 2) + '\n');
  return { verified: true, sourceId, nodeIds: targets, hasSecrets: (source.secrets || []).length > 0, sourceDigest: record.sourceDigest };
}

export async function writeIndex(output) {
  const records = [];
  let files;
  try { files = await readdir(join(output, 'manifests')); } catch (error) { if (error.code !== 'ENOENT') throw error; files = []; }
  for (const name of files.sort()) {
    if (!/^[a-f0-9]{32}\.json$/.test(name)) continue;
    records.push(JSON.parse(await readFile(join(output, 'manifests', name), 'utf8')));
  }
  const lines = ['# 我的 LLM Wiki', '', '由 AIO Memory 生成的净化快照。当前权限和最新知识请通过 AIO 智能体查询。', ''];
  const nodes = new Map(), edges = new Map();
  for (const record of records) {
    nodes.set(record.sourceId, { id: record.sourceId, kind: 'SOURCE', title: '净化来源' });
    for (const node of record.nodes) {
      id(node.id); id(record.sourceId);
      const title = node.title.replace(/[\r\n]/g, ' ').replace(/[\\[\]`*_<>]/g, '\\$&');
      lines.push(`- [${title}](entries/${record.sourceId}/${node.id}.md) · v${node.version}`);
      nodes.set(node.id, node);
    }
    for (const edge of record.edges) edges.set(edge.id, edge);
  }
  await privateWrite(join(output, 'index.md'), lines.join('\n') + '\n');
  await privateWrite(join(output, 'graph.json'), JSON.stringify({ nodes: [...nodes.values()], edges: [...edges.values()], snapshot: true }, null, 2) + '\n');
}
