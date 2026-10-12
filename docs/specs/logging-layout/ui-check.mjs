import {spawn} from 'node:child_process';
import {readFileSync} from 'node:fs';
import {createServer} from 'node:net';
import {fileURLToPath} from 'node:url';

async function unusedPort() {
  const probe=createServer();
  await new Promise((resolve,reject)=>{probe.once('error',reject);probe.listen(0,'127.0.0.1',resolve);});
  const address=probe.address();
  if(!address || typeof address==='string') throw Error('No loopback port');
  await new Promise((resolve,reject)=>probe.close(error=>error?reject(error):resolve()));
  return address.port;
}

async function main() {
  const cwd=fileURLToPath(new URL('../../../crates/ax-web/web-ui/',import.meta.url));
  const port=await unusedPort();
  const url=`http://127.0.0.1:${port}`;
  const expected=readFileSync(cwd+'dist/index.html','utf8');
  const server=spawn('node',['node_modules/vite/bin/vite.js','preview','--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd,stdio:'ignore'});
  let failure;server.on('error',e=>{failure=e;});server.on('exit',code=>{failure=Error(`Preview exited: ${code}`);});
  try {
    let ready=false;
    for(let i=0;i<100;i++) {
      if(failure) throw failure;
      let response;
      try { response=await fetch(url,{signal:AbortSignal.timeout(1000)}); } catch { /* Expected while preview starts. */ }
      if(response) {
        if(!response.ok || await response.text()!==expected) throw Error('Preview does not serve the freshly built index');
        ready=true;break;
      }
      await new Promise(resolve=>setTimeout(resolve,100));
    }
    if(!ready) throw Error('Vite preview did not become ready');
    const args=['exec','--','playwright','test','e2e/logging-layout.spec.ts','e2e/focus-row-density.spec.ts','e2e/admin-brand-home.spec.ts','e2e/control-spacing.spec.ts','--project=desktop-chrome','--project=mobile-chrome'];
    if(process.env.AX_DENSITY_GREP) args.push('--grep',process.env.AX_DENSITY_GREP);
    const child=spawn('npm',args,{cwd,env:{...process.env,AX_WEB_URL:url},stdio:'inherit'});
    process.exitCode=await new Promise(resolve=>{child.on('error',()=>resolve(1));child.on('exit',code=>resolve(code??1));});
  } finally { server.kill(); }
}
await main();
