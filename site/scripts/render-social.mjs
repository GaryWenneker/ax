/**
 * Render release announcement images for social platforms.
 *
 * Usage (from repo root or site/):
 *   node site/scripts/render-social.mjs
 *   AX_SOCIAL_VERSION=5.0.0 AX_SOCIAL_SHOT=cc-policy-skills.png node site/scripts/render-social.mjs
 *
 * If the bundled Chrome for Testing does not start, set PUPPETEER_EXECUTABLE_PATH to an installed Chrome.
 * Output: site/public/social/v<version>/ax-<version>-<format>.png at exact platform sizes.
 */
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { mkdirSync, writeFileSync, existsSync, rmSync, mkdtempSync } from 'node:fs';
import os from 'node:os';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const siteDir = path.resolve(__dirname, '..');
const require = createRequire(path.join(siteDir, 'scripts', 'render-social.mjs'));

const version = process.env.AX_SOCIAL_VERSION || '5.0.0';
const shotName = process.env.AX_SOCIAL_SHOT || 'cc-policy-skills.png';
const outDir = path.join(siteDir, 'public', 'social', `v${version}`);

const FORMATS = [
	{ name: 'og-1200x630', width: 1200, height: 630, layout: 'wide' },
	{ name: 'github-1280x640', width: 1280, height: 640, layout: 'wide' },
	{ name: 'square-1080x1080', width: 1080, height: 1080, layout: 'square' },
	{ name: 'story-1080x1920', width: 1080, height: 1920, layout: 'story' },
];

const HEADLINE = 'One copy of every rule and skill.';
const POINTS = [
	['Policy dedup', 'the longest copy wins, every change versioned'],
	['Read guard', 'agents use the graph before whole-file reads'],
	['Review loop', 'stack reviews until zero findings'],
	['PR comments', 'you choose each comment before it posts'],
];

const fileUrl = (p) => pathToFileURL(p).href;

function fontCss() {
	const pkgs = [
		'@fontsource/bebas-neue/index.css',
		'@fontsource-variable/archivo/index.css',
		'@fontsource/ibm-plex-mono/400.css',
	];
	return pkgs.map((p) => `<link rel="stylesheet" href="${fileUrl(require.resolve(p))}">`).join('\n');
}

function html(format) {
	const logo = fileUrl(path.join(siteDir, 'public', 'logo.png'));
	const shot = fileUrl(path.join(siteDir, 'public', 'screenshots', shotName));
	const points = POINTS.map(
		([title, text]) => `<li><b>${title}</b><span>${text}</span></li>`,
	).join('');
	return `<!doctype html><html><head><meta charset="utf-8">${fontCss()}<style>
:root { --paper:#121110; --paper2:#1a1814; --ink:#f3f1ea; --ink2:#b8b5a8; --accent:#c9a55c; }
* { box-sizing:border-box; margin:0; padding:0; }
html, body { width:${format.width}px; height:${format.height}px; overflow:hidden; }
body { background:radial-gradient(120% 90% at 85% 10%, #23211a 0%, var(--paper) 55%); color:var(--ink);
  font-family:'Archivo Variable', system-ui, sans-serif; display:flex; }
.logo { background:url('${logo}') no-repeat; background-size:200% auto; background-position:48% 45%;
  aspect-ratio:1.65; }
.tag { font-family:'IBM Plex Mono', monospace; color:var(--accent); letter-spacing:.08em; text-transform:uppercase; }
.version { font-family:'Bebas Neue', sans-serif; color:var(--accent); line-height:.9; }
h1 { font-family:'Bebas Neue', sans-serif; font-weight:400; line-height:.95; letter-spacing:.01em; }
ul { list-style:none; display:flex; flex-direction:column; }
li { display:flex; flex-direction:column; border-left:3px solid var(--accent); }
li b { font-weight:700; }
li span { color:var(--ink2); }
.frame { border:1px solid #2c2a23; border-radius:14px; overflow:hidden; background:var(--paper2);
  box-shadow:0 30px 80px rgba(0,0,0,.55); }
.frame .bar { height:28px; background:#1e1c16; display:flex; gap:8px; align-items:center; padding:0 12px; }
.frame .bar i { width:11px; height:11px; border-radius:50%; background:#57554c; }
.frame img { display:block; width:100%; height:auto; }
.url { font-family:'IBM Plex Mono', monospace; color:var(--ink2); }

/* wide: text left, screenshot right */
.wide { flex-direction:row; padding:56px 0 56px 64px; gap:40px; align-items:center; }
.wide .text { flex:0 0 46%; display:flex; flex-direction:column; gap:18px; }
.wide .logo { width:170px; margin-left:-12px; }
.wide .version { font-size:84px; }
.wide h1 { font-size:54px; }
.wide ul { gap:10px; }
.wide li { padding-left:12px; font-size:17px; }
.wide li span { font-size:15px; }
.wide .tag { font-size:15px; }
.wide .url { font-size:15px; }
.wide .shotwrap { flex:1; margin-right:-120px; transform:perspective(1600px) rotateY(-10deg); }

/* square and story: stacked */
.square, .story { flex-direction:column; align-items:flex-start; }
.square { padding:72px; gap:22px; }
.square .top { display:flex; align-items:center; gap:24px; }
.square .logo { width:200px; margin-left:-14px; }
.square .version { font-size:120px; }
.square h1 { font-size:76px; }
.square ul { gap:12px; }
.square li { padding-left:14px; font-size:24px; }
.square li span { font-size:20px; }
.square .tag { font-size:20px; }
.square .url { font-size:20px; }
.square .shotwrap { flex:1; min-height:0; overflow:hidden; width:118%; margin-top:8px; border-radius:14px; }

.story { padding:120px 84px 110px; gap:36px; }
.story .logo { width:330px; margin-left:-20px; }
.story .version { font-size:200px; }
.story h1 { font-size:116px; }
.story ul { gap:22px; }
.story li { padding-left:20px; font-size:36px; }
.story li span { font-size:30px; }
.story .tag { font-size:28px; }
.story .url { font-size:30px; }
.story .shotwrap { flex:1; min-height:0; overflow:hidden; width:125%; border-radius:14px; }
</style></head><body class="${format.layout}">
${format.layout === 'wide' ? wideBody(points, shot) : stackedBody(format.layout, points, shot)}
</body></html>`;
}

const frame = (shot) =>
	`<div class="shotwrap"><div class="frame"><div class="bar"><i></i><i></i><i></i></div><img src="${shot}"></div></div>`;

function wideBody(points, shot) {
	return `<div class="text"><div class="logo"></div><div class="tag">Major release</div>
<div class="version">v${version}</div><h1>${HEADLINE}</h1><ul>${points}</ul>
<div class="url">getax.wenneker.io</div></div>${frame(shot)}`;
}

function stackedBody(layout, points, shot) {
	const head =
		layout === 'square'
			? `<div class="top"><div class="logo"></div><div><div class="tag">Major release</div><div class="version">v${version}</div></div></div>`
			: `<div class="logo"></div><div class="tag">Major release</div><div class="version">v${version}</div>`;
	return `${head}<h1>${HEADLINE}</h1><ul>${points}</ul>${frame(shot)}<div class="url">getax.wenneker.io</div>`;
}

async function main() {
	if (!existsSync(path.join(siteDir, 'public', 'screenshots', shotName))) {
		throw new Error(`screenshot not found: ${shotName} (run npm run screenshots first)`);
	}
	mkdirSync(outDir, { recursive: true });
	const puppeteer = require('puppeteer');
	const tmp = mkdtempSync(path.join(os.tmpdir(), 'ax-social-'));
	const browser = await puppeteer.launch({ headless: true, args: ['--allow-file-access-from-files'] });
	try {
		const page = await browser.newPage();
		for (const format of FORMATS) {
			await page.setViewport({ width: format.width, height: format.height, deviceScaleFactor: 1 });
			const file = path.join(tmp, `${format.name}.html`);
			writeFileSync(file, html(format), 'utf8');
			await page.goto(fileUrl(file), { waitUntil: 'networkidle0' });
			await page.evaluate(() => document.fonts.ready);
			const out = path.join(outDir, `ax-${version}-${format.name}.png`);
			await page.screenshot({ path: out, type: 'png' });
			console.log(`${format.name} -> ${path.relative(siteDir, out)}`);
		}
	} finally {
		await browser.close();
		rmSync(tmp, { recursive: true, force: true });
	}
}

main().catch((err) => {
	console.error(err.message || err);
	process.exit(1);
});
