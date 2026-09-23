// Layout check for the docs header and the mobile menu button.
// Usage: node scripts/check-mobile-header.mjs [baseUrl]   (default http://localhost:4321)
// Exits 1 on any failed assertion, a page error, or a missing element.
import puppeteer from 'puppeteer';

const base = (process.argv[2] || 'http://localhost:4321').replace(/\/$/, '');
const pagePath = '/getting-started/introduction/';
const failures = [];
const fail = (msg) => failures.push(msg);

function luminance([r, g, b]) {
	const c = [r, g, b].map((v) => {
		const s = v / 255;
		return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
	});
	return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}
function contrast(a, b) {
	const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
	return (hi + 0.05) / (lo + 0.05);
}
function parseRgba(s) {
	const m = s.match(/rgba?\(([^)]+)\)/);
	if (!m) throw new Error(`unparseable colour: ${s}`);
	const [r, g, b, a = '1'] = m[1].split(',').map((x) => x.trim());
	return { rgb: [+r, +g, +b], a: +a };
}
function over(fg, bgRgb) {
	return fg.rgb.map((v, i) => Math.round(v * fg.a + bgRgb[i] * (1 - fg.a)));
}

async function measure(page) {
	return page.evaluate(() => {
		const rect = (el) => {
			if (!el) return null;
			const r = el.getBoundingClientRect();
			return { x: r.x, y: r.y, w: r.width, h: r.height, right: r.right };
		};
		const btn = document.querySelector('starlight-menu-button button');
		const icon = btn && [...btn.querySelectorAll('svg')].find((s) => getComputedStyle(s).display !== 'none');
		const cs = btn && getComputedStyle(btn);
		return {
			vw: document.documentElement.clientWidth,
			glass: rect(document.querySelector('.ax-header-glass')),
			inner: rect(document.querySelector('.ax-header-glass .header > *')),
			search: rect(document.querySelector('site-search button[data-open-modal]')),
			btn: rect(btn),
			btnVisible: !!btn && cs.display !== 'none' && cs.visibility !== 'hidden',
			btnBg: cs && cs.backgroundColor,
			iconColor: icon && getComputedStyle(icon).color,
			iconRect: rect(icon),
			expanded: btn && btn.getAttribute('aria-expanded'),
		};
	});
}

// Header glass is the darkest backdrop behind the button: rgba(6,6,16) at the top of the bar.
const GLASS = [6, 6, 16];

function checkMenuButton(m, label) {
	if (!m.btn || !m.btnVisible) return fail(`${label}: menu button not visible`);
	if (!m.iconColor || !m.iconRect || m.iconRect.w === 0) return fail(`${label}: no visible menu icon`);
	const bg = over(parseRgba(m.btnBg), GLASS);
	const icon = over(parseRgba(m.iconColor), bg);
	const ratio = contrast(icon, bg);
	if (ratio < 3) fail(`${label}: icon contrast ${ratio.toFixed(2)}:1 < 3:1 (bg ${m.btnBg}, icon ${m.iconColor})`);
}

const browser = await puppeteer.launch({ executablePath: process.env.PUPPETEER_EXECUTABLE_PATH });
try {
	const page = await browser.newPage();
	page.on('pageerror', (e) => fail(`page error: ${e.message}`));

	await page.setViewport({ width: 390, height: 844, deviceScaleFactor: 2, isMobile: true, hasTouch: true });
	const res = await page.goto(base + pagePath, { waitUntil: 'networkidle2' });
	if (!res || !res.ok()) throw new Error(`GET ${pagePath} -> ${res && res.status()}`);
	let m = await measure(page);
	if (!m.glass || !m.inner || !m.search) throw new Error('header elements missing');

	if (m.glass.x !== 0 || m.glass.y !== 0) fail(`mobile: glass starts at (${m.glass.x},${m.glass.y}), expected (0,0)`);
	if (Math.abs(m.glass.w - m.vw) > 0.5) fail(`mobile: glass width ${m.glass.w} != viewport ${m.vw}`);
	checkMenuButton(m, 'mobile closed');
	if (m.btn && m.search.right > m.btn.x - 4) fail(`mobile: search (right ${m.search.right}) overlaps menu button (x ${m.btn.x})`);

	await page.click('starlight-menu-button button');
	await page.waitForFunction(() => document.querySelector('starlight-menu-button')?.getAttribute('aria-expanded') === 'true');
	m = await measure(page);
	checkMenuButton(m, 'mobile open');

	await page.setViewport({ width: 1280, height: 800 });
	await page.goto(base + pagePath, { waitUntil: 'networkidle2' });
	m = await measure(page);
	if (m.glass.x !== 0 || Math.abs(m.glass.w - m.vw) > 0.5) fail(`desktop: glass not full width (x ${m.glass.x}, w ${m.glass.w}, vw ${m.vw})`);
	if (m.inner.x < 8) fail(`desktop: header content has no inline padding (x ${m.inner.x})`);
	if (m.btnVisible) fail('desktop: menu button should be hidden');
} finally {
	await browser.close();
}

if (failures.length) {
	console.error('FAIL');
	for (const f of failures) console.error('  - ' + f);
	process.exit(1);
}
console.log('PASS: mobile header full-bleed, menu button visible (closed + open), desktop layout intact');
