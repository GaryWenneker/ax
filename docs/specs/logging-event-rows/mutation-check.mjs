/** Run three actual mutants against the presentation contract in an isolated copy. */
import {readFile,writeFile,copyFile,mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawn} from 'node:child_process';
import assert from 'node:assert/strict';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const source=root+'crates/ax-web/web-ui/src/lib/loggingPresentation.ts';
const original=await readFile(source,'utf8');
const temporary=await mkdtemp(join(tmpdir(),'ax-log-presentation-'));
const module=join(temporary,'loggingPresentation.ts');
async function run() {
 const child=spawn(process.execPath,['--experimental-strip-types','--test',join(temporary,'loggingPresentation.test.ts')],{stdio:['ignore','pipe','pipe']});
 let output='';for(const stream of [child.stdout,child.stderr])stream.on('data',chunk=>output+=chunk);
 const code=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',resolve);});
 return {code,output};
}
try {
 await writeFile(join(temporary,'package.json'),'{"type":"module"}\n');
 await copyFile(root+'crates/ax-web/web-ui/src/lib/loggingPresentation.test.ts',join(temporary,'loggingPresentation.test.ts'));
 await writeFile(module,original);const baseline=await run();assert.equal(baseline.code,0,baseline.output);
 const mutants=[
  ['M1: lose action summaries',"if (action && (phase==='start' || phase==='ok'))",'if (false)'],
  ['M2: drop count/timing facts',"return facts.join(' · ');","return '';"],
  ['M3: rewrite human prompt text',"if (text) return {title: text, subtitle};",'if (false) return {title: text, subtitle};'],
 ];
 for(const [label,marker,replacement]of mutants) {
  assert.equal(original.split(marker).length,2,`Missing or ambiguous marker: ${label}`);
  await writeFile(module,original.replace(marker,replacement));
  const result=await run();assert.equal(result.code,1,result.output);assert.match(result.output,/AssertionError|ERR_ASSERTION/);
  console.log(`${label}: executed and killed by an assertion`);
 }
 assert.equal(await readFile(source,'utf8'),original,'Working source changed');
 console.log('Mutation gate passed: 3/3 real mutants killed; working source unchanged.');
} finally {await rm(temporary,{recursive:true,force:true});}
