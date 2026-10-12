/** Capture current website source locally; never builds or deploys Netlify. */
import {spawn} from 'node:child_process';
import {createServer} from 'node:net';
import {readFileSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import puppeteer from 'puppeteer';
const site=fileURLToPath(new URL('../',import.meta.url));
const probe=createServer();await new Promise((ok,no)=>{probe.once('error',no);probe.listen(0,'127.0.0.1',ok);});
const port=probe.address().port;await new Promise(ok=>probe.close(ok));
const server=spawn('node',['node_modules/astro/bin/astro.mjs','dev','--host','127.0.0.1','--port',String(port)],{cwd:site,stdio:['ignore','pipe','pipe']});
let startupOutput='';for(const stream of [server.stdout,server.stderr])stream.on('data',chunk=>startupOutput=(startupOutput+chunk).slice(-3000));
let browser;let exited;server.on('error',e=>exited=e);server.on('exit',code=>exited=Error(`Astro exited: ${code}`));
try {
 let ready=false;
 for(let i=0;i<300;i++) {
  if(exited)throw exited;
  try {if((await fetch(`http://127.0.0.1:${port}`,{signal:AbortSignal.timeout(1000)})).ok){ready=true;break;}} catch{}
  await new Promise(ok=>setTimeout(ok,100));
 }
 if(!ready)throw Error('Local website did not become ready: '+startupOutput);
 browser=await puppeteer.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage'],...(process.env.AX_CHROME_PATH?{executablePath:process.env.AX_CHROME_PATH}:{})});
 const page=await browser.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 const entries=[];
 const sourceHash=createHash('sha256').update(readFileSync(site+'src/pages/index.astro')).update(readFileSync(site+'src/styles/minimal-home.css')).digest('hex');
 for(const [name,width,height]of [['desktop',1440,900],['mobile',393,852]]) {
  await page.setViewport({width,height,deviceScaleFactor:1});await page.emulateMediaFeatures([{name:'prefers-reduced-motion',value:'reduce'}]);
  await page.goto(`http://127.0.0.1:${port}`,{waitUntil:'networkidle2'});await page.waitForSelector('main');
  if(errors.length)throw Error(errors.join('; '));
  const file=`home-install-${name}.png`;await page.screenshot({path:site+'public/screenshots/'+file});
  const imageHash=createHash('sha256').update(readFileSync(site+'public/screenshots/'+file)).digest('hex');
  entries.push({file,imageHash,route:'/',theme:'graphite',viewport:{width,height,deviceScaleFactor:1},data:'public-site',sourceHash,capturedAt:new Date().toISOString()});console.log(file);
 }
 const path=site+'public/screenshots/manifest.json';const prior=JSON.parse(readFileSync(path,'utf8'));const names=new Set(entries.map(e=>e.file));
 writeFileSync(path,JSON.stringify([...prior.filter(e=>!names.has(e.file)),...entries],null,2)+'\n');
} finally {if(browser)await browser.close();server.kill();}
