import {readFileSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {chromium} from '../../../crates/ax-web/web-ui/node_modules/playwright/index.mjs';
import {startScreenshotServer} from '../../../site/scripts/screenshot-fixtures.mjs';

const root=fileURLToPath(new URL('../../../',import.meta.url));
const directory=root+'site/public/screenshots/';
const index=readFileSync(root+'crates/ax-web/web-ui/dist/index.html');
const digest=createHash('sha256').update(index);
for(const asset of index.toString().matchAll(/(?:src|href)="(\/assets\/[^\"]+)"/g)) digest.update(readFileSync(root+'crates/ax-web/web-ui/dist'+asset[1]));
const sourceHash=digest.digest('hex');
const fixture=await startScreenshotServer(root+'crates/ax-web/web-ui/dist');
const browser=await chromium.launch({headless:true,executablePath:process.env.AX_CHROME_EXECUTABLE,args:['--no-sandbox']});
const shots=[
 {file:'cc-mobile-settings.png',route:'/settings'},
 {file:'cc-mobile-settings-sections.png',route:'/settings',action:page=>page.getByRole('button',{name:'Choose settings section'}).click()},
 {file:'cc-mobile-nodes.png',route:'/nodes'},
 {file:'cc-mobile-filters.png',route:'/nodes',action:page=>page.getByRole('button',{name:'Filters',exact:true}).click()},
 {file:'cc-mobile-details.png',route:'/nodes',action:page=>page.locator('.page-item').first().click()},
 {file:'cc-mobile-navigation.png',route:'/stats',action:page=>page.getByRole('button',{name:'Toggle menu'}).click()},
 {file:'cc-logging-dark-mobile.png',route:'/logging'},
 {file:'cc-mobile-logging-filters.png',route:'/logging',action:page=>page.getByRole('button',{name:'Show filters',exact:true}).click()},
 {file:'cc-mobile-sort-filter.png',route:'/nodes',action:page=>page.getByRole('button',{name:'Sort and filter',exact:true}).click()},
 {file:'cc-mobile-memory-form.png',route:'/memory',action:page=>page.getByRole('button',{name:'New memory',exact:true}).click()},
 {file:'cc-mobile-files-preview.png',route:'/files',action:async page=>{await page.locator('.file-tree-row--folder').first().click();await page.locator('.file-tree-row--file').first().click();}},
 {file:'cc-mobile-graph-settings.png',route:'/graph',action:page=>page.getByRole('button',{name:'Graph settings',exact:true}).click()},
 {file:'cc-mobile-savings.png',route:'/savings'},
 {file:'cc-mobile-review-filters.png',route:'/policy/review',action:page=>page.getByRole('button',{name:'Filters',exact:true}).click()},
 {file:'cc-mobile-review-details.png',route:'/policy/review',action:page=>page.getByRole('button',{name:'Inspect content and source',exact:true}).click()},
 {file:'cc-mobile-editor.png',route:'/policy/rules/edit'},
];
const entries=[];
try {
 for(const shot of shots) {
  const context=await browser.newContext({viewport:{width:390,height:844},deviceScaleFactor:1,isMobile:true,hasTouch:true});
  await context.addInitScript(()=>localStorage.setItem('ax-theme','azure-dark'));
  const page=await context.newPage();const errors=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('response',response=>{if(response.status()>=400) errors.push(new URL(response.url()).pathname+': '+response.status());});
  await page.goto(fixture.url+shot.route,{waitUntil:'domcontentloaded'});
  await page.locator('#main-content').waitFor({state:'visible'});
  if(shot.route==='/logging') await page.locator('.mcp-trace-row').first().waitFor();
  if(shot.action) await shot.action(page);
  await page.waitForTimeout(300);
  if(errors.length) throw Error(`${shot.file}: ${errors.join('; ')}`);
  const png=await page.screenshot({path:directory+shot.file});
  entries.push({file:shot.file,imageHash:createHash('sha256').update(png).digest('hex'),sourceHash,route:shot.route,state:{action:shot.action?'opened control':'page'},theme:'azure-dark',viewport:{width:390,height:844,deviceScaleFactor:1},viewportOrigin:'fixture-declared',imageSize:{width:png.readUInt32BE(16),height:png.readUInt32BE(20)},data:'public synthetic fixtures',capturedAt:new Date().toISOString()});
  console.log(shot.file);await context.close();
 }
 const manifest=JSON.parse(readFileSync(directory+'manifest.json','utf8'));
 const updated=new Set(entries.map(entry=>entry.file));
 writeFileSync(directory+'manifest.json',JSON.stringify([...manifest.filter(entry=>!updated.has(entry.file)),...entries],null,2)+'\n');
} finally {await browser.close();await new Promise(resolve=>fixture.server.close(resolve));}
