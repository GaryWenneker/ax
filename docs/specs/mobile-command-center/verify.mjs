import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {writeFileSync} from 'node:fs';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const results=[];
async function run(command,args,cwd=root,env=process.env) {
 const child=spawn(command,args,{cwd,env,stdio:'inherit'});
 const code=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('exit',code=>resolve(code??1));});
 results.push({command,args,cwd:cwd.slice(root.length),exitCode:code});
 writeFileSync(root+'docs/specs/mobile-command-center/verification-results.json',JSON.stringify(results,null,2)+'\n');
 if(code!==0) throw Error(`${command} ${args.join(' ')} exited ${code}`);
}
await run('npm',['run','build'],root+'crates/ax-web/web-ui');
await run(process.execPath,['docs/specs/mobile-command-center/unit-check.mjs']);
await run('npm',['test','--prefix','site']);
await run(process.execPath,['docs/specs/list-columns/ui-check.mjs'],root,{...process.env,AX_UI_DEMO:'1',AX_UI_PROJECTS:'desktop-chrome',AX_COLUMN_OUTPUT:root+'crates/ax-web/web-ui/test-results/mobile-desktop'});
await run(process.execPath,['docs/specs/list-columns/ui-check.mjs'],root,{...process.env,AX_UI_DEMO:'1',AX_UI_PROJECTS:'mobile-chrome',AX_UI_SPECS:'e2e/mobile-command-center.spec.ts',AX_COLUMN_OUTPUT:root+'crates/ax-web/web-ui/test-results/mobile-interactions'});
await run(process.execPath,['docs/specs/mobile-command-center/audit.mjs'],root,{...process.env,AX_AUDIT_STRICT:'1',AX_AUDIT_STAGE:'final'});
await run(process.execPath,['docs/specs/mobile-command-center/capture.mjs']);
await run(process.execPath,['site/scripts/audit-documentation.mjs']);
await run('git',['diff','--check','HEAD','--','.',':(exclude)crates/ax-web/web-ui/dist']);
console.log('Mobile verification complete');
