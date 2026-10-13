/** Local-only gauntlet; no backend, Docker, Netlify, releases or workflow dispatches. */
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const ui=root+'crates/ax-web/web-ui';
const npm=process.platform==='win32'?'npm.cmd':'npm';
const specs=['column-accessibility','list-columns','logging-layout','focus-row-density','admin-brand-home','control-spacing'].map(name=>`e2e/${name}.spec.ts`).join(',');
const checks=[
 ['build',npm,['run','build'],ui],
 ['unit','node',['--experimental-strip-types','--test','src/components/agent/activityModel.test.ts','src/lib/calmRows.test.ts','src/lib/routes.test.ts','src/lib/themes.test.ts','src/lib/loggingPresentation.test.ts'],ui],
 ['coverage','node',['--experimental-strip-types','--experimental-test-coverage','--test-coverage-include=**/loggingPresentation.ts','--test-coverage-lines=100','--test-coverage-functions=100','--test-coverage-branches=100','--test','src/lib/loggingPresentation.test.ts'],ui],
 ['mutation','node',['docs/specs/logging-event-rows/mutation-check.mjs'],root],
 ['browser','node',['docs/specs/list-columns/ui-check.mjs'],root],
 ['site',npm,['test'],root+'site'],
 ['audit','node',['site/scripts/audit-documentation.mjs'],root],
];
const names=process.argv.find(argument=>argument.startsWith('--checks='))?.slice(9).split(',');
if(names?.some(name=>!checks.some(check=>check[0]===name)))throw Error('Unknown check');
for(const [name,command,args,cwd]of checks.filter(check=>!names||names.includes(check[0]))) {
 console.log(`Running ${name}`);
 const child=spawn(command,args,{cwd,stdio:'inherit',env:{...process.env,AX_UI_DEMO:'1',AX_UI_SPECS:specs}});
 const code=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',resolve);});
 if(code!==0)throw Error(`${name} failed: ${code}`);
}
