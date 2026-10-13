import {build} from '../../../crates/ax-web/web-ui/node_modules/esbuild/lib/main.js';
import {readdirSync,mkdtempSync,rmSync,cpSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root=fileURLToPath(new URL('../../../',import.meta.url));
const output=mkdtempSync(join(tmpdir(),'ax-mobile-units-'));
try {
 cpSync(root+'crates/ax-web/web-ui/src',output,{recursive:true});
 const entryPoints=['src/lib','src/components/agent'].flatMap(dir=>readdirSync(root+'crates/ax-web/web-ui/'+dir).filter(file=>file.endsWith('.test.ts')).map(file=>root+'crates/ax-web/web-ui/'+dir+'/'+file));
 await build({entryPoints,bundle:true,platform:'node',format:'esm',outdir:output,outExtension:{'.js':'.mjs'},logLevel:'error'});
 const walk=dir=>readdirSync(dir,{withFileTypes:true}).flatMap(entry=>entry.isDirectory()?walk(join(dir,entry.name)):[join(dir,entry.name)]);
 const child=spawn(process.execPath,['--test','--test-reporter=tap',...walk(output).filter(file=>file.endsWith('.mjs'))],{stdio:'inherit'});
 process.exitCode=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('exit',code=>resolve(code??1));});
} finally {rmSync(output,{recursive:true,force:true});}
