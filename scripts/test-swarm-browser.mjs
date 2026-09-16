import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {chromium} from 'playwright';
import {writeFile} from 'node:fs/promises';
const port=14196;
const preview=spawn(process.execPath,['scripts/preview.mjs'],{env:{...process.env,PORT:String(port),AIO_AGENT_EXTERNAL_BACKEND:'1'},stdio:['ignore','pipe','pipe']});
let logs='';preview.stdout.on('data',b=>logs+=b);preview.stderr.on('data',b=>logs+=b);
const browser=await chromium.launch({channel:'chrome',headless:true});
const errors=[];const report=[];
try{
  for(let i=0;i<100;i++){
    try{if((await fetch(`http://127.0.0.1:${port}`)).ok){break;}}catch{}
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  for(const mobile of [false,true]){
    const context=await browser.newContext({viewport:mobile?{width:390,height:844}:{width:1440,height:900}});
    const page=await context.newPage();page.on('pageerror',error=>errors.push(error.message));
    await page.goto(`http://127.0.0.1:${port}`);
    const frame=page.frameLocator('#plugin');
    await frame.getByRole('button',{name:'蜂群任务',exact:true}).click();
    await frame.getByText('mini 检查 · Mac mini · 失败',{exact:true}).waitFor();
    await frame.getByText('book 检查 · MacBook · 已停止',{exact:true}).waitFor();
    await frame.getByText('mini 检查 · Mac mini · 失败',{exact:true}).click();
    const output=await frame.getByRole('textbox',{name:'mini 检查的执行结果'}).inputValue();
    assert.equal(JSON.parse(output).success,false);
    assert.equal(JSON.parse(output).jobs[0].result.exitCode,7);
    const metrics=await page.frames().find(f=>f.url().includes('/assets/')).evaluate(()=>({width:innerWidth,scroll:document.documentElement.scrollWidth}));
    assert(metrics.scroll<=metrics.width+1,'页面横向溢出');
    const path=`/tmp/aio-swarm-${mobile?'mobile':'desktop'}.png`;
    await page.frames().find(f=>f.url().includes('/assets/')).evaluate(async()=>{await Promise.all(document.getAnimations().map(a=>a.finished));});
    await page.screenshot({path});
    await frame.getByRole('button',{name:'关闭',exact:true}).click();
    await page.reload();
    await frame.getByRole('button',{name:'蜂群任务',exact:true}).click();
    await frame.getByText('mini 检查 · Mac mini · 失败',{exact:true}).waitFor();
    report.push({mobile,metrics,path,reloadRetained:true});
    await context.close();
  }
  assert.deepEqual(errors,[]);
  await writeFile('/tmp/aio-swarm-browser-result.json',JSON.stringify({report,errors},null,2));
  console.log(JSON.stringify({report,errors}));
}finally{
  await browser.close();preview.kill('SIGTERM');
  if(logs.includes('Backend stopped')){console.error(logs);}
}
