/**
 * Capture Command Center screenshots for getax.wenneker.io docs.
 *
 * Usage (from repo root or site/):
 *   node site/scripts/capture-screenshots.mjs
 *   AX_WEB_URL=http://127.0.0.1:7070 node site/scripts/capture-screenshots.mjs
 *
 * Requires: ax web running, and `npx puppeteer` (downloads Chromium on first run).
 */
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdirSync, existsSync } from 'node:fs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '../..');
const outDir = path.join(root, 'site', 'public', 'screenshots');
const baseUrl = (process.env.AX_WEB_URL || 'http://127.0.0.1:7070').replace(/\/$/, '');

const VIEWPORT = { width: 1440, height: 900, deviceScaleFactor: 2 };

/** @type {Array<{ name: string, path: string, waitMs?: number, ready?: string, after?: (page: import('puppeteer').Page) => Promise<void> }>} */
const SHOTS = [
	{ name: 'cc-ship-full.png', path: '/ship', waitMs: 1800, ready: '.ship-page, .page-ship, main' },
	{ name: 'cc-stats.png', path: '/stats', waitMs: 1200 },
	{ name: 'cc-nodes.png', path: '/nodes', waitMs: 1500 },
	{ name: 'cc-graph.png', path: '/graph', waitMs: 8000, ready: 'canvas' },
	{ name: 'cc-files.png', path: '/files', waitMs: 1500 },
	{ name: 'cc-search.png', path: '/search', waitMs: 1200 },
	{ name: 'cc-memory-vault.png', path: '/memory', waitMs: 1200 },
	{
		name: 'cc-savings-dashboard.png',
		path: '/savings',
		waitMs: 3500,
		ready: '.sv-page, .savings-page, [class*="sv-"]',
		after: async (page) => {
			// show_savings loads async — click sidebar if we landed on Stats
			const ok = await page.evaluate(() => {
				if (document.querySelector('.sv-page, .savings-page, .sv-area-chart, .activity-heatmap')) return true;
				const link = Array.from(document.querySelectorAll('nav a, .nav-item, button, [role="button"]')).find(
					(el) => (el.textContent || '').trim() === 'Savings',
				);
				if (link instanceof HTMLElement) {
					link.click();
					return false;
				}
				return false;
			});
			if (!ok) await new Promise((r) => setTimeout(r, 2500));
			await page
				.waitForFunction(
					() =>
						!!document.querySelector(
							'.sv-page, .savings-page, .sv-area-chart, .activity-heatmap, [class*="heatmap"]',
						) || document.body.innerText.includes('Tokens saved'),
					{ timeout: 15000 },
				)
				.catch(() => {});
			await new Promise((r) => setTimeout(r, 1500));
		},
	},
	{ name: 'cc-logging.png', path: '/logging', waitMs: 2000, ready: '.logging-page, main' },
	{
		name: 'cc-mcp-quality.png',
		path: '/logging',
		waitMs: 1500,
		after: async (page) => {
			await page.evaluate(() => {
				window.dispatchEvent(new CustomEvent('ax-mcp-quality-open'));
			});
			await new Promise((r) => setTimeout(r, 1400));
			const hasSlide = await page.$('.mcp-q-blade, .mcp-q-overlay');
			if (!hasSlide) {
				await page.evaluate(() => {
					const btn =
						document.querySelector('.status-quality') ||
						document.querySelector('[aria-label*="MCP quality"]');
					if (btn instanceof HTMLElement) btn.click();
				});
				await new Promise((r) => setTimeout(r, 1200));
			}
		},
	},
	{ name: 'cc-settings.png', path: '/settings', waitMs: 1200 },
	{ name: 'cc-unresolved.png', path: '/unresolved', waitMs: 1500 },
	{ name: 'cc-policy-rules.png', path: '/policy/rules', waitMs: 1200 },
	{ name: 'cc-policy-skills.png', path: '/policy/skills', waitMs: 1200 },
	{ name: 'cc-policy-match.png', path: '/policy/match', waitMs: 1200 },
	{ name: 'cc-agent-terminal.png', path: '/agent', waitMs: 1200 },
];

async function loadPuppeteer() {
	const siteDir = path.join(root, 'site');
	const require = createRequire(path.join(siteDir, 'scripts', 'capture-screenshots.mjs'));
	try {
		return require('puppeteer');
	} catch {
		const { execSync } = await import('node:child_process');
		console.log('Installing puppeteer (one-time)...');
		execSync('npm install --no-save --no-package-lock puppeteer@24', {
			cwd: siteDir,
			stdio: 'inherit',
		});
		return require('puppeteer');
	}
}

async function waitReady(page, selector, timeoutMs = 15000) {
	if (!selector) return;
	try {
		await page.waitForSelector(selector, { timeout: timeoutMs });
	} catch {
		// Soft fail — still screenshot whatever rendered.
	}
}

async function main() {
	mkdirSync(outDir, { recursive: true });
	const puppeteer = await loadPuppeteer();

	console.log(`Base URL: ${baseUrl}`);
	console.log(`Output:   ${outDir}`);

	const browser = await puppeteer.launch({
		headless: true,
		defaultViewport: VIEWPORT,
		args: ['--no-sandbox', '--disable-dev-shm-usage', '--window-size=1440,900'],
	});

	const page = await browser.newPage();
	await page.setViewport(VIEWPORT);

	// Probe server
	try {
		const res = await page.goto(`${baseUrl}/`, { waitUntil: 'domcontentloaded', timeout: 30000 });
		if (!res || !res.ok()) throw new Error(`HTTP ${res?.status()}`);
		await new Promise((r) => setTimeout(r, 1500));
	} catch (err) {
		await browser.close();
		console.error(`ax web not reachable at ${baseUrl}: ${err.message}`);
		console.error('Start it with: ax web --port 7070');
		process.exit(1);
	}

	for (const shot of SHOTS) {
		const url = `${baseUrl}${shot.path}`;
		process.stdout.write(`→ ${shot.name} (${shot.path}) ... `);
		await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 45000 });
		await waitReady(page, shot.ready);
		if (shot.waitMs) await new Promise((r) => setTimeout(r, shot.waitMs));
		if (shot.after) await shot.after(page);

		const out = path.join(outDir, shot.name);
		await page.screenshot({ path: out, type: 'png', fullPage: false });
		console.log(existsSync(out) ? 'ok' : 'MISSING');
	}

	await browser.close();
	console.log(`\nDone. ${SHOTS.length} screenshots in ${outDir}`);
}

main().catch((err) => {
	console.error(err);
	process.exit(1);
});
