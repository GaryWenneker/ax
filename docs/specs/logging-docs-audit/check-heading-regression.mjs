/** Negative control: a renamed Logging heading must fail the browser contract. */
import {mkdtemp,cp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawn} from 'node:child_process';
import assert from 'node:assert/strict';
import {startScreenshotServer} from '../../../site/scripts/screenshot-fixtures.mjs';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const temporary=await mkdtemp(join(tmpdir(),'ax-heading-control-'));
let fixture;
try {
 await cp(root+'crates/ax-web/web-ui/dist',temporary,{recursive:true});
 const index=await readFile(join(temporary,'index.html'),'utf8');
 const asset=index.match(/src="(\/assets\/[^\"]+\.js)"/);assert.ok(asset,'Current JS bundle missing');
 const file=join(temporary,asset[1]);const js=await readFile(file,'utf8');
 const marker='className:"mcp-trace-heading",children:"Logging"';assert.ok(js.includes(marker),'Heading marker missing');
 await writeFile(file,js.replace(marker,'className:"mcp-trace-heading",children:"Trace"'));
 fixture=await startScreenshotServer(temporary);
 const child=spawn('npm',['exec','--','playwright','test','e2e/logging-layout.spec.ts','--project=desktop-chrome','--grep','compact page heading','--reporter=list'],{cwd:root+'crates/ax-web/web-ui',env:{...process.env,AX_WEB_URL:fixture.url},stdio:['ignore','pipe','pipe']});
 let output='';for(const stream of [child.stdout,child.stderr])stream.on('data',chunk=>output+=chunk);
 const code=await new Promise((ok,no)=>{child.once('error',no);child.once('exit',ok);});
 assert.equal(code,1,output);assert.match(output,/heading.*Logging/);assert.match(output,/toBeVisible/);
 console.log('Negative control passed: the Logging heading regression is rejected by the browser test.');
} finally {
 if(fixture){fixture.server.closeAllConnections();await new Promise(ok=>fixture.server.close(ok));}
 await rm(temporary,{recursive:true,force:true});
}
