import {readFileSync, mkdirSync, writeFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {chromium} from '../../../crates/ax-web/web-ui/node_modules/playwright/index.mjs';
import {startScreenshotServer} from '../../../site/scripts/screenshot-fixtures.mjs';

const root=fileURLToPath(new URL('../../../',import.meta.url));
const source=readFileSync(root+'crates/ax-web/web-ui/src/lib/routes.ts','utf8');
const paths=[...source.slice(source.indexOf('const PAGE_PATH'),source.indexOf('const PATH_PAGE')).matchAll(/:\s*'(\/[^']+)'/g)].map(m=>m[1]);
const settings=[...readFileSync(root+'crates/ax-web/web-ui/src/lib/settingsNavigation.ts','utf8').matchAll(/id: '([^']+)'/g)].map(m=>m[1]);
if (!paths.length || !settings.length) throw Error('Route or Settings inventory is empty');
const targets=process.env.AX_AUDIT_PATHS?.split(',')??[...paths,...settings.map(id=>`/settings?section=${id}`)];
const out=root+'docs/specs/mobile-command-center/'+(process.env.AX_AUDIT_STAGE??'final');
mkdirSync(out,{recursive:true});
const fixture=await startScreenshotServer(root+'crates/ax-web/web-ui/dist');
const browser=await chromium.launch({headless:true,executablePath:process.env.AX_CHROME_EXECUTABLE,args:['--no-sandbox']});
const results=[];
try {
 for(const width of (process.env.AX_AUDIT_WIDTHS??'320,390,768,1280').split(',').map(Number)) {
  const context=await browser.newContext({viewport:{width,height:844},isMobile:width<900,hasTouch:width<900});
  await context.addInitScript(()=>localStorage.setItem('ax-theme','azure-dark'));
  for(const path of targets) {
   const page=await context.newPage();const errors=[];const missing=[];
   page.on('pageerror',e=>errors.push(e.message));
   page.on('response',r=>{if(r.status()>=400) missing.push(new URL(r.url()).pathname+':'+r.status());});
   await page.goto(fixture.url+path,{waitUntil:'domcontentloaded'});
   await page.locator('#main-content').waitFor({state:'visible'});
   await page.waitForTimeout(350);
   if (process.env.AX_AUDIT_NEGATIVE_CONTROL==='1') await page.locator('#main-content').evaluate(el => el.style.width='calc(100% + 400px)');
   const geometry=await page.evaluate(()=>{
    const main=document.querySelector('#main-content');
    const rect=main.getBoundingClientRect();
    const clipped=[...main.querySelectorAll('input,select,button,.page-item,.settings-row')].filter(el=>{
      const r=el.getBoundingClientRect();
      const closed=el.closest('details:not([open])');
      if (closed && !closed.querySelector('summary')?.contains(el)) return false;
      let ancestor=el.parentElement;
      while(ancestor && ancestor!==main) {
        if (['auto','scroll'].includes(getComputedStyle(ancestor).overflowX) && ancestor.scrollWidth>ancestor.clientWidth+2) return false;
        ancestor=ancestor.parentElement;
      }
      return r.width>0 && r.height>0 && getComputedStyle(el).visibility!=='hidden' && !el.closest('[hidden]') && (r.left<rect.left-2 || r.right>rect.right+2);
    }).slice(0,15).map(el=>({tag:el.tagName,class:el.className,text:el.textContent?.trim().slice(0,50)}));
    return {viewport:innerWidth,documentWidth:document.documentElement.scrollWidth,mainWidth:rect.width,mainScrollWidth:main.scrollWidth,clipped};
   });
   const record={width,path,actualPath:new URL(page.url()).pathname,errors,missing,...geometry};results.push(record);
   if(width===390) await page.screenshot({path:out+'/'+path.replaceAll('/','_').replaceAll('?','_').replaceAll('=','-')+'.png',fullPage:true});
   console.log(JSON.stringify(record));
   await page.close();
  }
  await context.close();
 }
 writeFileSync(out+'/results.json',JSON.stringify({provenance:'compiled UI, synthetic public API fixtures',targets,results},null,2));
} finally {await browser.close();await new Promise(resolve=>fixture.server.close(resolve));}
if(process.env.AX_AUDIT_STRICT==='1' && results.some(r=>r.errors.length || r.missing.length || r.clipped.length || r.documentWidth>r.width || r.mainScrollWidth>r.mainWidth+2 || r.mainWidth>r.width)) process.exitCode=1;
