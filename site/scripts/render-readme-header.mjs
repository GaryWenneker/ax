/**
 * Render docs/images/readme-header.webp, the banner at the top of the README,
 * from the logo master in assets/logo-light.svg.
 *
 * Run from site/: node scripts/render-readme-header.mjs
 */
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { createRequire } from 'module';
import { fileURLToPath, pathToFileURL } from 'url';
import { dirname, join } from 'path';

const require = createRequire(import.meta.url);
const puppeteer = require('puppeteer');
const sharp = require('sharp');

const repo = join(dirname(fileURLToPath(import.meta.url)), '../..');
const WIDTH = 1672;
const HEIGHT = 941;

const mark = readFileSync(join(repo, 'assets/logo-light.svg'), 'utf8');
const fonts = ['@fontsource-variable/oxanium/wght.css', '@fontsource-variable/archivo/index.css', '@fontsource/ibm-plex-mono/400.css']
	.map((name) => `<link rel="stylesheet" href="${pathToFileURL(require.resolve(name))}">`)
	.join('\n');

const html = `<!doctype html><html><head><meta charset="utf-8">${fonts}<style>
* { box-sizing: border-box; margin: 0; padding: 0; }
html, body { width: ${WIDTH}px; height: ${HEIGHT}px; overflow: hidden; }
body { background: #080a09; color: #f0f3ed; font-family: 'Archivo Variable', system-ui, sans-serif; position: relative; }
.dots { position: absolute; inset: 0;
  background-image: radial-gradient(circle, #a2f50f 2.6px, transparent 3.2px); background-size: 26px 26px;
  -webkit-mask-image: radial-gradient(62% 78% at 82% 42%, rgba(0,0,0,.62) 0%, rgba(0,0,0,.2) 55%, transparent 100%);
  mask-image: radial-gradient(62% 78% at 82% 42%, rgba(0,0,0,.62) 0%, rgba(0,0,0,.2) 55%, transparent 100%); }
.content { position: absolute; left: 132px; top: 0; bottom: 0; display: flex; flex-direction: column; justify-content: center; }
.eyebrow { font-family: 'IBM Plex Mono', monospace; font-size: 22px; letter-spacing: .16em; color: #a2f50f; margin-bottom: 44px; }
.mark svg { display: block; height: 236px; width: auto; }
h1 { font-family: 'Oxanium Variable', sans-serif; font-weight: 500; font-size: 116px; line-height: 1; letter-spacing: -.02em; margin-top: 52px; }
p { font-family: 'Oxanium Variable', sans-serif; font-weight: 500; font-size: 50px; line-height: 1.2; color: #a2f50f; margin-top: 18px; }
ul { list-style: none; display: flex; gap: 14px; margin-top: 52px; font-family: 'IBM Plex Mono', monospace; font-size: 20px; letter-spacing: .1em; }
li { border: 1.5px solid #a2f50f; border-radius: 999px; padding: 9px 20px; color: #d8f7a0; }
</style></head><body>
<div class="dots"></div>
<div class="content">
  <div class="eyebrow">LOCAL-FIRST / OPEN SOURCE / DEVELOPER TOOLING</div>
  <div class="mark">${mark}</div>
  <h1>Agent Xpansion</h1>
  <p>The context layer for AI agents.</p>
  <ul><li>CONTEXT</li><li>RULES</li><li>MEMORY</li><li>ONE NATIVE BINARY</li></ul>
</div>
</body></html>`;

// The page is loaded from a file so that it may load the local font files.
const work = mkdtempSync(join(tmpdir(), 'ax-readme-header-'));
const pagePath = join(work, 'header.html');
writeFileSync(pagePath, html);
const browser = await puppeteer.launch({ args: ['--allow-file-access-from-files'] });
try {
	const page = await browser.newPage();
	await page.setViewport({ width: WIDTH, height: HEIGHT, deviceScaleFactor: 1 });
	await page.goto(pathToFileURL(pagePath).href, { waitUntil: 'networkidle0' });
	await page.evaluate(() => document.fonts.ready);
	const shot = await page.screenshot({ type: 'png' });
	const output = join(repo, 'docs/images/readme-header.webp');
	await sharp(shot).webp({ quality: 90 }).toFile(output);
	console.log(`Wrote ${output}`);
} finally {
	await browser.close();
	rmSync(work, { recursive: true, force: true });
}
