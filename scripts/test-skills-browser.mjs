import assert from 'node:assert/strict';
import {execFileSync,spawn} from 'node:child_process';
import {createServer} from 'node:http';
import {readFile,mkdtemp,rm} from 'node:fs/promises';
import {join,resolve,extname} from 'node:path';
import {tmpdir} from 'node:os';
import {chromium} from 'playwright';
const directory=await mkdtemp(join(tmpdir(),'aio-skills-browser-'));
execFileSync(process.execPath,['scripts/setup-dev.mjs'],{env:{...process.env,AIO_AGENT_DEV_DIRECTORY:directory},stdio:'inherit'});
const config=JSON.parse(await readFile(join(directory,'runtime.json'),'utf8'));
const port=24194;
const backend=spawn('target/debug/az-agent-server',[],{env:{...process.env,AIO_PLUGIN_PORT:String(port),AIO_AGENT_DATABASE_URL:config.databaseUrl,AIO_AGENT_MASTER_KEY:config.masterKey,AIO_AGENT_INGRESS_TOKEN:config.ingressToken,AIO_AGENT_ENDPOINTS:''},stdio:['ignore','ignore','pipe']});
const root=resolve('dist/frontend');
const server=createServer(async(req,res)=>{
 try {
  if(req.url==='/test/request'){
   const chunks=[];for await(const c of req)chunks.push(c);const body=JSON.parse(Buffer.concat(chunks));
   const response=await fetch(`http://127.0.0.1:${port}${body.path}`,{method:body.method,headers:{'content-type':'application/json','x-aio-token':config.ingressToken,'x-aio-tenant-id':'browser-test','x-aio-user-id':'owner','x-aio-context':'interactive'},body:JSON.stringify(body.body)});
   res.writeHead(response.status,{'content-type':response.headers.get('content-type') || 'application/json'}).end(await response.text());return;
  }
  const pathname=decodeURIComponent(new URL(req.url,'http://local').pathname);const file=resolve(root,'.'+pathname);
  if(!file.startsWith(root+'/')){res.writeHead(403).end();return;}
  let bytes=await readFile(file);
  if(pathname.endsWith('.html'))bytes=Buffer.from(bytes.toString().replace('<head>',`<head><script>window.aioPlugin={json:async(method,path,body)=>{const r=await fetch('/test/request',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({method,path,body})});const v=await r.json();if(!r.ok)throw Error(v.error||'失败');return v;}};</script>`));
  res.setHeader('content-type',({'.html':'text/html; charset=utf-8','.js':'application/javascript','.wasm':'application/wasm','.css':'text/css','.woff2':'font/woff2'})[extname(file)] || 'application/octet-stream');res.end(bytes);
 }catch{res.writeHead(404).end();}
});
let browser;
try {
 for(let i=0;;i++){try{if((await fetch(`http://127.0.0.1:${port}/health`)).ok)break;}catch{}if(i>100)throw Error('backend startup timeout');await new Promise(r=>setTimeout(r,100));}
 await new Promise(r=>server.listen(0,'127.0.0.1',r));
 browser=await chromium.launch({headless:true});const page=await browser.newPage({viewport:{width:1440,height:900}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(`http://127.0.0.1:${server.address().port}/skills.html`);
 await page.getByRole('heading',{name:'Skill 管理',exact:true}).waitFor();
 await page.getByRole('button',{name:'新建 Skill',exact:true}).click();
 await page.getByRole('textbox',{name:'Skill 文件路径',exact:true}).fill('browser-acceptance/SKILL.md');
 await page.getByRole('textbox',{name:'Skill 正文',exact:true}).fill('---\nname: browser-acceptance\ndescription: browser test\n---\n\nBrowser creates a skill.');
 await page.getByRole('button',{name:'保存',exact:true}).click();
 await page.getByRole('cell',{name:'browser-acceptance',exact:true}).waitFor();
 await page.getByRole('button',{name:'管理',exact:true}).click();
 await page.getByRole('textbox',{name:'Skill 正文',exact:true}).waitFor();
 await page.waitForFunction(()=>document.querySelector('[aria-label="Skill 正文"]')?.value.includes('Browser creates'));
 await page.getByRole('textbox',{name:'Skill 正文',exact:true}).fill('Updated in the browser');
 await page.getByRole('button',{name:'保存',exact:true}).click();
 await page.getByRole('dialog').waitFor({state:'hidden'});
 await page.screenshot({path:'/tmp/aio-skills-desktop.png',fullPage:true});
 const desktop=await page.evaluate(()=>({viewport:innerWidth,width:document.documentElement.scrollWidth}));assert.ok(desktop.width<=desktop.viewport+1);
 await page.setViewportSize({width:390,height:844});await page.screenshot({path:'/tmp/aio-skills-mobile.png',fullPage:true});
 const mobile=await page.evaluate(()=>({viewport:innerWidth,width:document.documentElement.scrollWidth}));assert.ok(mobile.width<=mobile.viewport+1);
 await page.getByRole('button',{name:'管理',exact:true}).click();
 await page.waitForFunction(()=>document.querySelector('[aria-label="Skill 正文"]')?.value==='Updated in the browser');
 await page.getByRole('button',{name:'删除当前文件',exact:true}).click();
 await page.getByRole('button',{name:'确认删除',exact:true}).click();
 await page.getByRole('cell',{name:'browser-acceptance',exact:true}).waitFor({state:'hidden'});
 assert.deepEqual(errors,[]);console.log('PASS desktop/mobile: create, edit, delete confirmation, no overflow, no runtime errors; screenshots /tmp/aio-skills-{desktop,mobile}.png');
}finally{
 await browser?.close();await new Promise(r=>server.close(r));backend.kill('SIGTERM');await new Promise(r=>backend.once('exit',r));await rm(directory,{recursive:true,force:true});
}
