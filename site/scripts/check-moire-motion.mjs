import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import puppeteer from 'puppeteer';
import sharp from 'sharp';

const url = process.env.AX_SITE_URL ?? 'http://127.0.0.1:4322';
assert.ok(['127.0.0.1','localhost'].includes(new URL(url).hostname),'Use a local preview');
const output = '/tmp/ax-moire-local';
await mkdir(output,{recursive:true});
const browser = await puppeteer.launch({executablePath:process.env.AX_CHROME_EXECUTABLE,headless:true,args:['--no-sandbox']});
try {
  for (const width of [1440,390]) {
    const page = await browser.newPage();
    await page.setViewport({width,height:900});
    await page.emulateMediaFeatures([{name:'prefers-reduced-motion',value:'no-preference'}]);
    await page.goto(url,{waitUntil:'networkidle0'});
    await page.waitForSelector('.ax-halftone-screen-two');
    const transform = () => page.$eval('.ax-halftone-screen-two',el=>getComputedStyle(el).transform);
    const first = await transform();
    const before = await page.screenshot({path:`${output}/${width}-before.png`});
    await new Promise(resolve=>setTimeout(resolve,1200));
    const second = await transform();
    const after = await page.screenshot({path:`${output}/${width}-after.png`});
    assert.notEqual(first,second,`${width}: background must move`);
    const a = await sharp(before).removeAlpha().raw().toBuffer();
    const b = await sharp(after).removeAlpha().raw().toBuffer();
    let changed = 0;
    for (let i=0;i<a.length;i+=3) if (Math.abs(a[i]-b[i])+Math.abs(a[i+1]-b[i+1])+Math.abs(a[i+2]-b[i+2])>24) changed++;
    const changeRatio = changed/(a.length/3);
    assert.ok(changeRatio>.002,`${width}: visible movement (${changeRatio})`);
    await page.click('[data-ax-motion-toggle]');
    await page.waitForFunction(()=>document.documentElement.dataset.axMotion==='paused');
    await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
    const paused = await transform();
    await new Promise(resolve=>setTimeout(resolve,300));
    assert.equal(await transform(),paused,`${width}: Pause must freeze motion`);
    await page.emulateMediaFeatures([{name:'prefers-reduced-motion',value:'reduce'}]);
    await page.click('[data-ax-motion-toggle]');
    const play = await transform();
    await new Promise(resolve=>setTimeout(resolve,300));
    assert.notEqual(await transform(),play,`${width}: explicit Play must override reduced motion`);
    await page.reload({waitUntil:'networkidle0'});
    assert.equal(await page.$eval('html',el=>el.dataset.axMotion),'running');
    await page.evaluate(()=>localStorage.removeItem('ax-motion-choice'));
    await page.reload({waitUntil:'networkidle0'});
    assert.equal(await page.$eval('html',el=>el.dataset.axMotion),'paused');
    console.log(`PASS ${width}px: ${Math.round(changeRatio*100)}% of pixels visibly changed; Pause, Play override, reload and reduced-motion default verified`);
    await page.close();
  }
} finally { await browser.close(); }
