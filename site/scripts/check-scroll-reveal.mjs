import assert from 'node:assert/strict';
import puppeteer from 'puppeteer';
import { prepareShareShot } from '../src/lib/og-card.mjs';

const url = process.env.AX_SITE_URL ?? 'http://127.0.0.1:4322';
assert.ok(['127.0.0.1', 'localhost'].includes(new URL(url).hostname), 'Use a local preview');
const browser = await puppeteer.launch({ executablePath: process.env.AX_CHROME_EXECUTABLE, headless: true, args: ['--no-sandbox'] });
try {
  for (const width of [1440, 390]) {
    const page = await browser.newPage();
    await page.setViewport({ width, height: 320 });
    await page.emulateMediaFeatures([{ name: 'prefers-reduced-motion', value: 'no-preference' }]);
    await page.goto(url, { waitUntil: 'networkidle0' });
    assert.ok(await page.$$eval('[data-ax-reveal].ax-reveal', items => items.length >= 9), `${width}: all homepage sections mount reveal`);
    assert.ok(await page.$$eval('.ax-reveal-pending', items => items.length > 0), `${width}: offscreen sections await scroll`);
    await page.waitForFunction(() => getComputedStyle(document.querySelector('.ax-download')).opacity === '0');
    assert.equal(await page.$eval('.ax-download', el => getComputedStyle(el).opacity), '0', `${width}: offscreen download starts hidden`);
    await page.$eval('.ax-download', el => el.scrollIntoView());
    await page.waitForFunction(() => !document.querySelector('.ax-download').classList.contains('ax-reveal-pending'));
    await page.waitForFunction(() => getComputedStyle(document.querySelector('.ax-download')).opacity === '1');
    await page.goto(url, { waitUntil: 'networkidle0' });
    await page.$eval('[data-ax-download]', el => el.focus({ preventScroll: true }));
    assert.equal(await page.$eval('.ax-download', el => el.classList.contains('ax-reveal-pending')), false, 'Keyboard focus exposes controls');
    await page.$eval('[data-ax-motion-toggle]', el => el.click());
    assert.equal(await page.$$eval('.ax-reveal-pending', items => items.length), 0, 'Pause exposes all content');
    await page.evaluate(() => localStorage.removeItem('ax-motion-choice'));
    await page.emulateMediaFeatures([{ name: 'prefers-reduced-motion', value: 'reduce' }]);
    await page.reload({ waitUntil: 'networkidle0' });
    assert.equal(await page.$$eval('.ax-reveal-pending', items => items.length), 0, 'Reduced motion exposes all content');
    await page.$eval('[data-ax-motion-toggle]', el => el.click());
    assert.notEqual(await page.$eval('.ax-reveal', el => getComputedStyle(el).transitionDuration), '0s', 'Explicit Play permits reveal transitions under reduced motion');
    await page.evaluate(prepareShareShot);
    assert.equal(await page.$eval('.ax-download', el => getComputedStyle(el).opacity), '1', 'Share-image preparation exposes reveal content');
    await page.setJavaScriptEnabled(false);
    await page.reload({ waitUntil: 'networkidle0' });
    assert.equal(await page.$eval('.ax-download', el => getComputedStyle(el).opacity), '1', 'No JavaScript keeps download visible');
    await page.close();
    console.log(`${width}px: reveal, scroll, focus, Pause, reduced motion, explicit Play, share-image visibility and no-JavaScript passed`);
  }
} finally { await browser.close(); }
