import assert from 'node:assert/strict';
import {execFileSync,spawn} from 'node:child_process';
import {readFile,mkdtemp,mkdir,writeFile,rm,realpath} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {randomUUID} from 'node:crypto';
import pg from 'pg';

const directory=await mkdtemp(join(tmpdir(),'aio-skills-service-'));
execFileSync(process.execPath,['scripts/setup-dev.mjs'],{env:{...process.env,AIO_AGENT_DEV_DIRECTORY:directory},stdio:'inherit'});
const config=JSON.parse(await readFile(join(directory,'runtime.json'),'utf8'));
const port=24193;
const child=spawn('target/debug/az-agent-server',[],{env:{...process.env,AIO_PLUGIN_PORT:String(port),AIO_AGENT_DATABASE_URL:config.databaseUrl,AIO_AGENT_MASTER_KEY:config.masterKey,AIO_AGENT_INGRESS_TOKEN:config.ingressToken,AIO_AGENT_ENDPOINTS:''},stdio:['ignore','ignore','pipe']});
let logs='';child.stderr.on('data',d=>logs+=d);
async function request(body,user='owner',tenant='test',worker=false,context='interactive') {
 const response=await fetch(`http://127.0.0.1:${port}/${worker?'worker/skills.sync':'skills'}`,{method:'POST',headers:{'content-type':'application/json','x-aio-token':config.ingressToken,'x-aio-tenant-id':tenant,'x-aio-user-id':user,'x-aio-context':context},body:JSON.stringify(body)});
 return {status:response.status,body:await response.text().then(s=>{try{return JSON.parse(s);}catch{return s;}})};
}
const db=new pg.Client({connectionString:config.databaseUrl});
try {
 for(let i=0;;i++){try{const r=await fetch(`http://127.0.0.1:${port}/health`);if(r.ok)break;}catch{}if(child.exitCode!==null || i>100)throw Error(`server startup failed ${logs}`);await new Promise(r=>setTimeout(r,100));}
 await db.connect();
 const payload={operation:'write',path:'demo/SKILL.md',expected:null,content:Buffer.from('secret skill fixture').toString('base64'),executable:false};
 const first=await request(payload);assert.equal(first.status,200);const hash=first.body.hash;
 assert.equal((await request({...payload,content:Buffer.from('other').toString('base64')})).status,409);
 assert.equal((await request(payload)).status,200,'identical retry is idempotent');
 for(const [user,tenant] of [['other','test'],['owner','other']]){assert.deepEqual((await request({operation:'list'},user,tenant)).body.files,[]);assert.equal((await request({operation:'read',path:payload.path},user,tenant)).status,404);}
 assert.equal((await request({...payload,path:'demo/../../secret'})).status,400);
 assert.equal((await request({...payload,userId:'other'})).status,422);
 assert.equal((await request({operation:'list'},'owner','test',true)).status,400);
 const encrypted=(await db.query('SELECT content FROM agent_skill_files WHERE path=$1',[payload.path])).rows[0].content;
 assert.ok(!encrypted.includes(Buffer.from('secret skill fixture')));
 const updates=await Promise.all(['one','two'].map(s=>request({...payload,expected:hash,content:Buffer.from(s).toString('base64')})));
 assert.deepEqual(updates.map(r=>r.status).sort(),[200,409]);
 const device=randomUUID(),context=`worker:${device}`;
 const status=await request({operation:'status',report:{conflicts:[{path:payload.path,local:'local',remote:'remote'}],phase:'conflict'}},'owner','test',true,context);assert.equal(status.status,200);
 assert.equal((await request({operation:'resolve',device,path:payload.path,side:'local',local:'old',remote:'remote'})).status,409);
 assert.equal((await request({operation:'resolve',device,path:payload.path,side:'local',local:'local',remote:'remote'})).status,200);
 // 真实数据库、HTTP 服务和客户端文件同步器连起来，覆盖两个方向及基线重试。
 const {build}=await import('../../aio-plugin-space/node_modules/esbuild/lib/main.js');
 const output=join(directory,'sync.mjs');await build({entryPoints:['../aio-plugin-space/src/skills/sync.ts'],bundle:true,platform:'node',format:'esm',outfile:output});
 const {synchronize}=await import(output);
 const root=await realpath(directory);await mkdir(join(root,'local-skill'));await writeFile(join(root,'local-skill/SKILL.md'),'from device');
 const base={};const remote=async body=>{const r=await request(body,'sync-user','test',true,context);assert.equal(r.status,200);return r.body;};
 let report=await synchronize(root,device,base,remote,join(directory,'backups'),async()=>{});assert.equal(report.uploaded,1);
 const cloud=(await remote({operation:'list'})).files[0];
 await remote({operation:'write',path:cloud.path,expected:cloud.hash,content:Buffer.from('from web').toString('base64'),executable:false});
 report=await synchronize(root,device,base,remote,join(directory,'backups'),async()=>{});assert.equal(report.downloaded,1);
 assert.equal(await readFile(join(root,cloud.path),'utf8'),'from web');
 console.log('PASS skills: owner/tenant isolation, identity injection, encrypted persistence, CAS race, conflict resolution, real HTTP + database + filesystem two-way sync');
} finally {
 await db.end().catch(()=>{});child.kill('SIGTERM');await new Promise(r=>child.once('exit',r));
 await rm(directory,{recursive:true,force:true});
}
