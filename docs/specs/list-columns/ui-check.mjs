import {screenshotResponse} from '../../../site/scripts/screenshot-fixtures.mjs';
import {spawn} from 'node:child_process';
import {readFileSync} from 'node:fs';
import {createServer} from 'node:net';
import {createServer as createHttpServer} from 'node:http';
import {Readable} from 'node:stream';
import {fileURLToPath} from 'node:url';

async function unusedPort() {
  const probe=createServer();
  await new Promise((resolve,reject)=>{probe.once('error',reject);probe.listen(0,'127.0.0.1',resolve);});
  const address=probe.address();
  if(!address || typeof address==='string') throw Error('No loopback port');
  await new Promise((resolve,reject)=>probe.close(error=>error?reject(error):resolve()));
  return address.port;
}

async function streamFixture(upstream) {
  // One Playwright worker owns this loopback-only network fixture.
  const clients = new Set();
  let lines=[];
  const server = createHttpServer(async(req,res) => {
    res.setHeader('Content-Type','text/plain; charset=utf-8');
    try {
      const requestUrl=new URL(req.url,'http://127.0.0.1');
      if(requestUrl.pathname==='/configure') {
        const chunks=[];for await(const chunk of req) chunks.push(chunk);
        const data=JSON.parse(Buffer.concat(chunks).toString('utf8'));
        if(!Array.isArray(data.lines) || !data.lines.every(line=>typeof line==='string')) {res.writeHead(400);res.end('Invalid fixture');return;}
        lines=data.lines;res.end('ok');return;
      }
      if(requestUrl.pathname==='/emit') {
        const line=requestUrl.searchParams.get('line')??'';
        if(!clients.size) {res.writeHead(404);res.end('Stream not connected');return;}
        for(const client of clients) client.write(`event: line\ndata: ${line}\n\n`);
        res.end('ok');return;
      }
      if(requestUrl.pathname==='/api/usage/mcp-trace/events') {
        res.writeHead(200,{'Content-Type':'text/event-stream; charset=utf-8','Cache-Control':'no-cache'});
        clients.add(res);res.write(`event: batch\ndata: ${JSON.stringify(lines)}\n\n`);
        const heartbeat=setInterval(()=>res.write(': heartbeat\n\n'),1000);
        res.on('close',()=>{clearInterval(heartbeat);clients.delete(res);});return;
      }
      if(process.env.AX_UI_DEMO === '1' && requestUrl.pathname.startsWith('/api/')) {
        const response=screenshotResponse(requestUrl.href);
        res.writeHead(response.status??200,{'Content-Type':response.contentType??'application/json'});
        res.end(response.body??JSON.stringify(response.json));return;
      }
      const response=await fetch(upstream+req.url,{signal:AbortSignal.timeout(10000)});
      res.writeHead(response.status,{'Content-Type':response.headers.get('content-type')??'application/octet-stream'});
      if(response.body) {const stream=Readable.fromWeb(response.body);stream.on('error',()=>res.destroy());stream.pipe(res);} else res.end();
    } catch {if(res.headersSent) res.destroy();else {res.writeHead(502);res.end('Fixture proxy failed');}}
  });
  await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(0,'127.0.0.1',resolve);});
  const address=server.address();
  if(!address || typeof address==='string') throw Error('No fixture port');
  return {url:`http://127.0.0.1:${address.port}`,close:async()=>{for(const client of clients) client.destroy();await new Promise(resolve=>server.close(resolve));}};
}

async function main() {
  const cwd=fileURLToPath(new URL('../../../crates/ax-web/web-ui/',import.meta.url));
  const port=await unusedPort();
  const url=`http://127.0.0.1:${port}`;
  const expected=readFileSync(cwd+'dist/index.html','utf8');
  const server=spawn('node',['node_modules/vite/bin/vite.js','preview','--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd,stdio:'ignore'});
  const fixture=await streamFixture(url);
  let failure;server.on('error',e=>{failure=e;});server.on('exit',code=>{failure=Error(`Preview exited: ${code}`);});
  try {
    let ready=false;
    for(let i=0;i<100;i++) {
      if(failure) throw failure;
      let response;
      try { response=await fetch(fixture.url,{signal:AbortSignal.timeout(1000)}); } catch { /* Expected while preview starts. */ }
      if(response && response.status!==502) {
        if(!response.ok || await response.text()!==expected) throw Error('Preview does not serve the freshly built index');
        ready=true;break;
      }
      await new Promise(resolve=>setTimeout(resolve,100));
    }
    if(!ready) throw Error('Vite preview did not become ready');
    const specs=process.env.AX_UI_SPECS ? process.env.AX_UI_SPECS.split(',') : ['e2e/column-accessibility.spec.ts','e2e/list-columns.spec.ts','e2e/logging-layout.spec.ts','e2e/focus-row-density.spec.ts','e2e/admin-brand-home.spec.ts','e2e/control-spacing.spec.ts'];
    const args=['exec','--','playwright','test',...specs,'--project=desktop-chrome','--project=mobile-chrome'];
    if(process.env.AX_COLUMN_OUTPUT) args.push('--output',process.env.AX_COLUMN_OUTPUT,'--reporter=list');
    if(process.env.AX_DENSITY_GREP) args.push('--grep',process.env.AX_DENSITY_GREP);
    const captureStartedAt=Date.now();
    const child=spawn('npm',args,{cwd,env:{...process.env,AX_WEB_URL:fixture.url,AX_SSE_FIXTURE_URL:fixture.url},stdio:'inherit'});
    process.exitCode=await new Promise(resolve=>{child.on('error',()=>resolve(1));child.on('exit',code=>resolve(code??1));});
    if(process.exitCode===0 && process.env.AX_SCREENSHOT_MANIFEST === '1') {
      const record=spawn('node',[fileURLToPath(new URL('../../../site/scripts/record-test-screenshots.mjs',import.meta.url)),String(captureStartedAt)],{stdio:'inherit'});
      process.exitCode=await new Promise(resolve=>{record.on('error',()=>resolve(1));record.on('exit',code=>resolve(code??1));});
    }
  } finally { server.kill();await fixture.close(); }
}
await main();
