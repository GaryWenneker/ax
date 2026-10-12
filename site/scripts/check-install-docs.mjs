import assert from 'node:assert/strict';
import puppeteer from 'puppeteer';
const url = process.env.AX_SITE_URL ?? 'http://127.0.0.1:4322';
assert.ok(['localhost','127.0.0.1'].includes(new URL(url).hostname));
const browser = await puppeteer.launch({executablePath:process.env.AX_CHROME_EXECUTABLE,headless:true,args:['--no-sandbox']});
try {
  for (const width of [1440,390]) {
    const page = await browser.newPage();
    await page.setViewport({width,height:900});
    await page.evaluateOnNewDocument(() => {Object.defineProperty(navigator,'clipboard',{value:{writeText:async value=>{window.copiedInstall=value;}}});});
    await page.goto(url,{waitUntil:'networkidle0'});
    await page.addStyleTag({content:'astro-dev-toolbar{display:none!important}'});
    for (const os of ['macos','linux','windows','wsl2']) {
      await page.locator('[data-ax-platform-toggle]').click();
      await page.locator(`[data-ax-os="${os}"]`).click();
      const expected = os === 'windows' ? 'irm https://getax.wenneker.io/install.ps1 | iex' : 'curl -fsSL https://getax.wenneker.io/install.sh | sh';
      assert.equal(await page.$eval('[data-ax-install-code]',el=>el.textContent),expected);
      assert.equal(await page.$eval('[data-ax-download]',el=>el.getAttribute('href')),'#ax-install-command');
      console.log(`${width}: checking ${os}`);
      await page.locator('[data-ax-install-copy]').click();
      await page.waitForFunction(expected=>window.copiedInstall===expected,{},expected);
      await page.waitForFunction(()=>document.querySelector('[data-ax-install-status]').textContent.includes('copied'));
    }
    await page.evaluate(()=>{Object.defineProperty(navigator.clipboard,'writeText',{value:async()=>{throw Error('denied');}});});
    await page.locator('[data-ax-install-copy]').click();
    await page.waitForFunction(()=>document.querySelector('[data-ax-install-status]').textContent.includes('Select and copy'));
    await page.goto(url+'/getting-started/introduction/',{waitUntil:'networkidle0'});
    await page.waitForSelector('.ax-doc-moire .ax-halftone-screen-two');
    const motion=()=>page.$eval('.ax-doc-moire .ax-halftone-screen-two',el=>getComputedStyle(el).transform);
    const first=await motion();
    await page.waitForFunction(first=>getComputedStyle(document.querySelector('.ax-doc-moire .ax-halftone-screen-two')).transform!==first,{},first);
    for(const theme of ['dark','light']) {
      await page.evaluate(theme=>{document.documentElement.dataset.theme=theme;},theme);
      const bg=await page.$eval('.main-pane .content-panel',el=>getComputedStyle(el).backgroundColor);
      assert.match(bg,/rgba/);
      assert.ok(Number(bg.split(',').at(-1).replace(')',''))>=.88,bg);
      const ink=await page.$eval('.sl-markdown-content',el=>getComputedStyle(el).color);
      assert.equal(ink, theme==='light'?'rgb(32, 40, 29)':'rgb(240, 243, 237)');
    }
    assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
    await page.close();
    console.log(`${width}: all four installer platforms, copy/fallback and shared docs motion/overlay passed`);
  }
} catch (error) {const pages = await browser.pages(); await pages.at(-1).screenshot({path:'/tmp/ax-install-failure.png',fullPage:true}); console.log(await pages.at(-1).$eval('[data-ax-install-status]',el=>el.textContent)); throw error;} finally {await browser.close();}
