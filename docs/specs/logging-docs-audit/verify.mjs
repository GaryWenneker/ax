/** Local-only verification. Browser checks use an isolated preview and synthetic APIs. */
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const ui=root+'crates/ax-web/web-ui';const npm=process.platform==='win32'?'npm.cmd':'npm';
const specs=['column-accessibility','list-columns','logging-layout','focus-row-density','admin-brand-home','control-spacing','remote-host','remote-mcp-publish','replication-setup','review-compatibility','policy-review-density'].map(s=>`e2e/${s}.spec.ts`).join(',');
const checks=[
 ['build',npm,['run','build'],ui],
 ['unit','node',['--experimental-strip-types','--test','src/components/agent/activityModel.test.ts','src/lib/calmRows.test.ts','src/lib/routes.test.ts','src/lib/themes.test.ts'],ui],
 ['browser','node',['docs/specs/list-columns/ui-check.mjs'],root],
 ['site',npm,['test'],root+'site'],
 ['audit','node',['site/scripts/audit-documentation.mjs'],root],
 ['heading','node',['docs/specs/logging-docs-audit/check-heading-regression.mjs'],root],
];
const requested=process.argv.find(a=>a.startsWith('--checks='))?.slice(9).split(',');
if(requested?.some(name=>!checks.some(c=>c[0]===name)))throw Error('Unknown check');
for(const [name,command,args,cwd]of checks.filter(c=>!requested||requested.includes(c[0]))) {
 console.log(`Running ${name}`);
 const child=spawn(command,args,{cwd,stdio:'inherit',env:{...process.env,AX_UI_DEMO:'1',AX_UI_SPECS:specs}});
 const code=await new Promise((ok,no)=>{child.once('error',no);child.once('exit',ok);});
 if(code!==0)throw Error(`${name} failed: ${code}`);
}
