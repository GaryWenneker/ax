import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root=fileURLToPath(new URL('../../../',import.meta.url));
const output=root+'docs/specs/mobile-command-center/mutation';mkdirSync(output,{recursive:true});
const cases=[
 {id:'escape',file:'src/lib/overlayFocus.ts',from:"event.key === 'Escape'",to:"event.key === 'DisabledEscape'",test:'mobile navigation is hidden'},
 {id:'detail-width',file:'src/mobile.css',from:'width:100vw!important',to:'width:60vw!important',test:'mobile detail is a viewport'},
 {id:'form-focus',file:'src/components/ModalShell.tsx',from:'return containOverlayFocus(panel.current, () => close.current());',to:'return () => {};',test:'mobile forms fit small screens'},
];
async function run(command,args,cwd,env=process.env) {
 let log='';const child=spawn(command,args,{cwd,env});
 child.stdout.on('data',chunk=>{log+=chunk;});child.stderr.on('data',chunk=>{log+=chunk;});
 const code=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('exit',code=>resolve(code??1));});
 return {code,log};
}
const records=[];
try {
 for(const mutant of cases) {
  const file=root+'crates/ax-web/web-ui/'+mutant.file;const original=readFileSync(file,'utf8');
  if(original.split(mutant.from).length!==2) throw Error(`Mutation anchor is not unique: ${mutant.id}`);
  try {
   writeFileSync(file,original.replace(mutant.from,mutant.to));
   const build=await run('npm',['run','build'],root+'crates/ax-web/web-ui');
   if(build.code!==0) throw Error(`Mutant failed to compile: ${mutant.id}\n${build.log}`);
   const tested=await run(process.execPath,['docs/specs/list-columns/ui-check.mjs'],root,{...process.env,AX_UI_DEMO:'1',AX_UI_PROJECTS:'mobile-chrome',AX_UI_SPECS:'e2e/mobile-command-center.spec.ts',AX_DENSITY_GREP:mutant.test,AX_COLUMN_OUTPUT:output+'/'+mutant.id});
   writeFileSync(output+'/'+mutant.id+'.log',tested.log);
   if(tested.code!==1 || !/1 failed/.test(tested.log)) throw Error(`Mutant survived or harness failed: ${mutant.id}, exit ${tested.code}`);
   records.push({id:mutant.id,test:mutant.test,buildExit:build.code,testExit:tested.code,killed:true});
   console.log(`${mutant.id}: killed by ${mutant.test}`);
  } finally {writeFileSync(file,original);}
 }
 writeFileSync(output+'/results.json',JSON.stringify(records,null,2)+'\n');
} finally {
 const restored=await run('npm',['run','build'],root+'crates/ax-web/web-ui');
 if(restored.code!==0) throw Error(`Restored build failed: ${restored.log}`);
}
