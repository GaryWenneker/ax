/** Record only PNGs written by the just-completed isolated browser test run. */
import {readFileSync,writeFileSync,readdirSync,statSync,existsSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../../',import.meta.url));
const since=Number(process.argv[2]);if(!Number.isFinite(since)||since<=0)throw Error('Pass the test start time in milliseconds');
const dir=root+'site/public/screenshots/';
const index=readFileSync(root+'crates/ax-web/web-ui/dist/index.html');
const digest=createHash('sha256').update(index);
for(const asset of index.toString().matchAll(/(?:src|href)="(\/assets\/[^\"]+)"/g))digest.update(readFileSync(root+'crates/ax-web/web-ui/dist'+asset[1]));
const sourceHash=digest.digest('hex');
const entries=[];
const testFiles=new Set(['cc-logging-columns.png','cc-logging-columns-mobile.png','cc-accent-colors.png','cc-policy-review-desktop-chrome.png','cc-policy-review-mobile-chrome.png','cc-policy-review-detail.png','cc-review-compact-desktop.png','cc-review-compact-mobile.png','cc-review-comparison.png','cc-remote-host.png','mcp-local-and-remote.png','remote-mcp-project-desktop.png','remote-mcp-project-mobile.png','mcp-import-existing-server.png','cc-replication-configured.png','cc-primary-pairing.png','cc-savings-aligned.png']);
for(const theme of ['azure-light','azure-dark'])for(const control of ['local','replication'])testFiles.add(`cc-${control}-controls-${theme}.png`);
for(const file of readdirSync(dir).filter(f=>testFiles.has(f))) {
 const stat=statSync(dir+file);if(stat.mtimeMs<since)continue;
 const png=readFileSync(dir+file);if(png.subarray(1,4).toString()!=='PNG')throw Error(`Invalid PNG: ${file}`);
 const imageSize={width:png.readUInt32BE(16),height:png.readUInt32BE(20)};
 let viewport={width:1280,height:800,deviceScaleFactor:1};
 if(['cc-policy-review-mobile-chrome.png','cc-review-compact-mobile.png'].includes(file))viewport={width:393,height:851,deviceScaleFactor:2.75};
 if(file==='cc-logging-columns.png')viewport={width:1366,height:900,deviceScaleFactor:1};
 if(file==='cc-logging-columns-mobile.png')viewport={width:393,height:900,deviceScaleFactor:1};
 if(file==='cc-replication-configured.png')viewport={width:1440,height:1300,deviceScaleFactor:1};
 if(file==='cc-primary-pairing.png')viewport={width:1440,height:1800,deviceScaleFactor:1};
 if(file==='cc-savings-aligned.png')viewport={width:2048,height:900,deviceScaleFactor:1};
 if(['remote-mcp-project-desktop.png','mcp-local-and-remote.png'].includes(file))viewport={width:1280,height:1150,deviceScaleFactor:1};
 if(file==='remote-mcp-project-mobile.png')viewport={width:393,height:1700,deviceScaleFactor:2.75};
 const lightShots=['mcp-import-existing-server.png','mcp-local-and-remote.png','remote-mcp-project-desktop.png','remote-mcp-project-mobile.png'];
 const theme=lightShots.includes(file)||file.endsWith('azure-light.png')?'azure-light':'azure-dark';
 let route='/settings';let state={section:'remote',fixture:'browser-test'};
 if(/logging/.test(file)){route='/logging';state={fixture:'logging-layout'};}
 else if(/review/.test(file)){route='/policy/review';state={fixture:'policy-review',comparison:/detail|comparison/.test(file)};}
 else if(/-controls-/.test(file)){state={section:file.startsWith('cc-local')?'remote':'nodes',fixture:'control-spacing',configured:file.startsWith('cc-replication')};}
 else if(/replication|pairing/.test(file)){state={section:'nodes',fixture:'replication-setup',configured:/configured/.test(file)};}
 else if(/accent/.test(file)){state={section:'appearance',fixture:'accent-selection'};}
 else if(/savings/.test(file)){route='/savings';state={fixture:'savings-density'};}
 else if(/focus|responsive/.test(file)){route='/policy/rules';state={fixture:'focus-row-density'};}
 const imageHash=createHash('sha256').update(png).digest('hex');
 entries.push({file,imageHash,route,state,theme,viewport,viewportOrigin:'fixture-declared',imageSize,data:'synthetic',sourceHash,capturedAt:new Date(stat.mtimeMs).toISOString()});
}
const path=dir+'manifest.json',prior=existsSync(path)?JSON.parse(readFileSync(path,'utf8')):[];
const changed=new Set(entries.map(e=>e.file));writeFileSync(path,JSON.stringify([...prior.filter(e=>!changed.has(e.file)),...entries],null,2)+'\n');
console.log(`Recorded ${entries.length} fresh test screenshots`);
