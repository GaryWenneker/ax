/** Structural coverage only; semantic walkthrough and image review are separate gates. */
import {readFileSync,existsSync,readdirSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import GithubSlugger from 'github-slugger';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const read=p=>readFileSync(resolve(root,p),'utf8');
const walk=dir=>readdirSync(resolve(root,dir),{withFileTypes:true}).flatMap(e=>e.isDirectory()?walk(`${dir}/${e.name}`):[`${dir}/${e.name}`]);
const docs=walk('site/src/content/docs').filter(p=>p.endsWith('.md'));
const texts=new Map(docs.map(p=>[p,read(p)]));
const all=[...texts.values()].join('\n');
const issues=[];
function requireMention(value,text,location){if(!text.includes(value))issues.push(`${location}: missing ${value}`);}
if(process.argv.includes('--negative-control')) {
 requireMention('/missing-route','Existing documentation','control');
 assert.equal(issues.length,1);throw Error('Expected negative control: undocumented route rejected');
}
const pages=[...read('crates/ax-web/web-ui/src/lib/routes.ts').matchAll(/^  [\w'-]+: '(\/[^']*)',$/gm)].map(m=>m[1]);
const settings=[...read('crates/ax-web/web-ui/src/lib/settingsNavigation.ts').matchAll(/id: '([^']+)'/g)].map(m=>m[1]);
const guide=read('site/src/content/docs/guides/command-center-walkthrough.md');
for(const p of pages)requireMention('`'+p+'`',guide,'page matrix');
for(const id of settings)requireMention('`'+id+'`',guide,'Settings matrix');
const cliSource=read('crates/ax-cli/src/main.rs').split('enum Commands {')[1].split('\n#[derive')[0];
const commands=[...cliSource.matchAll(/^    ([A-Z][A-Za-z0-9]+)\b/gm)].map(m=>m[1].replace(/[A-Z]/g,(c,i)=>(i?'-':'')+c.toLowerCase())).filter(c=>!['serve','prompt-hook','session-hook'].includes(c));
const cli=read('site/src/content/docs/reference/cli.md');
for(const command of commands)requireMention('ax '+command,cli,'CLI reference');
const tools=[...new Set([...read('crates/ax-mcp/src/tools.rs').matchAll(/"name":\s*"(ax_\w+)"/g)].map(m=>m[1]))];
for(const tool of tools)requireMention(tool,read('site/src/content/docs/reference/mcp-server.md'),'MCP reference');
const api=read('site/src/content/docs/reference/web-api.md');
function body(module,name) {
 const file=`crates/ax-web/src/${module.replaceAll('::','/')}.rs`;
 const source=read(file),at=source.indexOf(`fn ${name}(`);
 if(at<0) throw Error(`${file}: router ${name} missing`);
 const tail=source.slice(at);return tail.slice(0,tail.indexOf('\n}')+2);
}
const router=read('crates/ax-web/src/workspace_state.rs');
const mounts=[...router.matchAll(/\.nest\(\s*"([^"\n]+)",\s*(?:crate::)?([\w:]+)::(\w+)\(/g)].map(m=>({prefix:m[1],module:m[2],fn:m[3]}));
mounts.push({prefix:'/api',module:'workspace_state',fn:'graph_router'});
const endpoints=new Set(['/api/reset-client-cache']);
function collect(prefix,module,fn) {
 const source=body(module,fn);
 for(const delegated of source.matchAll(/^\s*(router_\w+)\(/gm))collect(prefix,module,delegated[1]);
 for(const m of source.matchAll(/\.route\(\s*"([^"\n]+)"/g))endpoints.add((prefix+m[1]).replace(/\/$/,''));
 for(const m of source.matchAll(/\.nest\(\s*"([^"\n]+)",\s*crate::([\w:]+)::(\w+)\(/g))collect(prefix+m[1],m[2],m[3]);
}
for(const m of mounts)collect(m.prefix,m.module,m.fn);
for(const endpoint of endpoints)requireMention(endpoint,api,'HTTP reference');
let assets=0,links=0;
const referenced=new Set();
for(const [file,raw]of texts) {
 const text=raw.replace(/```[\s\S]*?```/g,'').replace(/`[^`\n]*`/g,'');
 for(const m of text.matchAll(/!?\[[^\]]*\]\((\/[^\s)]+|#[^\s)]+)\)/g)) {
  const [pathname,fragment]=m[1].split('#');
  const link=pathname.replace(/\/$/,'');
  if(!link&&!fragment)continue;
  if(link.startsWith('/screenshots/')) {assets++;referenced.add(link.slice('/screenshots/'.length));}
  else links++;
  const content=link?`site/src/content/docs${link}.md`:file,publicFile=`site/public${link}`;
  if(fragment && texts.has(content)) {
    const slugger=new GithubSlugger();
    const headings=[...texts.get(content).replace(/```[\s\S]*?```/g,'').matchAll(/^#{1,6} (.+)$/gm)].map(m=>slugger.slug(m[1].replace(/\[([^\]]+)\]\([^)]*\)/g,'$1').replace(/[`*]/g,'')));
    if(!headings.includes(decodeURIComponent(fragment)))issues.push(`${file}: broken fragment ${m[1]}`);
  }
  if(!existsSync(resolve(root,content))&&!existsSync(resolve(root,publicFile))&&!existsSync(resolve(root,`site/src/pages${link}.astro`)))issues.push(`${file}: broken ${link}`);
 }
}
const manifestPath=resolve(root,'site/public/screenshots/manifest.json');
if(existsSync(manifestPath)) {
 const manifest=JSON.parse(readFileSync(manifestPath,'utf8'));
 for(const name of referenced) {
  const entry=manifest.find(e=>e.file===name);
  if(!entry)issues.push(`screenshot provenance missing: ${name}`);
  else {
   const hash=createHash('sha256').update(readFileSync(resolve(root,'site/public/screenshots',name))).digest('hex');
   if(entry.imageHash!==hash)issues.push(`screenshot hash missing/stale: ${name}`);
   if(!entry.sourceHash || !entry.state && !entry.data.startsWith('public'))issues.push(`screenshot source/state missing: ${name}`);
  }
 }
} else issues.push('screenshot manifest missing');
console.log(JSON.stringify({pages:pages.length,settings:settings.length,cli:commands.length,mcp:tools.length,http:endpoints.size,documents:docs.length,links,screenshotReferences:assets,uniqueScreenshots:referenced.size,issues},null,2));
if(issues.length)process.exitCode=1;
