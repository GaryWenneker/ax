/**
 * After `astro build`, screenshot every page that publishes an og:image and
 * write a 1200×630 card. The version label comes from the built CLI source.
 *
 * Fails the build if any published image is missing or the wrong size.
 */
import { createRequire } from 'node:module';
import http from 'node:http';
import { createReadStream, existsSync, mkdirSync, readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { readSourceVersion } from '../src/lib/source-version.mjs';
import {
	cardHtml,
	clipBox,
	distPathForOgUrl,
	kickerFor,
	ogImagePath,
	pathnameFromDistHtml,
	prepareShareShot,
	readShareMeta,
	releaseLabel,
} from '../src/lib/og-card.mjs';

const siteDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const distDir = path.join(siteDir, 'dist');
const SITE = 'https://getax.wenneker.io';
const VIEWPORT = { width: 1000, height: 800 };

const TYPES = {
	'.html': 'text/html; charset=utf-8',
	'.css': 'text/css; charset=utf-8',
	'.js': 'text/javascript',
	'.mjs': 'text/javascript',
	'.png': 'image/png',
	'.jpg': 'image/jpeg',
	'.jpeg': 'image/jpeg',
	'.webp': 'image/webp',
	'.svg': 'image/svg+xml',
	'.woff': 'font/woff',
	'.woff2': 'font/woff2',
	'.json': 'application/json',
	'.txt': 'text/plain; charset=utf-8',
};

function walkHtml(dir, out = []) {
	for (const name of readdirSync(dir)) {
		const full = path.join(dir, name);
		if (statSync(full).isDirectory()) walkHtml(full, out);
		else if (name.endsWith('.html')) out.push(full);
	}
	return out;
}

function serve(root) {
	const rootWithSep = root.endsWith(path.sep) ? root : root + path.sep;
	return new Promise((resolve) => {
		const server = http.createServer((req, res) => {
			try {
				const url = new URL(req.url ?? '/', 'http://127.0.0.1');
				let rel = decodeURIComponent(url.pathname);
				if (rel.endsWith('/')) rel += 'index.html';
				const file = path.resolve(root, rel.replace(/^\/+/, ''));
				if (file !== root && !file.startsWith(rootWithSep)) {
					res.writeHead(403);
					res.end();
					return;
				}
				if (!existsSync(file) || statSync(file).isDirectory()) {
					res.writeHead(404);
					res.end();
					return;
				}
				res.writeHead(200, { 'Content-Type': TYPES[path.extname(file)] || 'application/octet-stream' });
				createReadStream(file).pipe(res);
			} catch {
				res.writeHead(500);
				res.end();
			}
		});
		server.listen(0, '127.0.0.1', () => {
			const addr = server.address();
			if (!addr || typeof addr === 'string') throw new Error('static server has no port');
			resolve({ server, port: addr.port });
		});
	});
}

function pagesToRender() {
	const byImage = new Map();
	for (const file of walkHtml(distDir)) {
		const html = readFileSync(file, 'utf8');
		const meta = readShareMeta(html);
		if (!meta.image) continue;
		const pathname = pathnameFromDistHtml(distDir, file);
		const expected = new URL(ogImagePath(pathname), SITE).href;
		if (meta.image !== expected) {
			throw new Error(`${file} og:image is ${meta.image}, expected ${expected}`);
		}
		if (!byImage.has(meta.image)) {
			byImage.set(meta.image, { pathname, title: meta.title, description: meta.description });
		}
	}
	if (byImage.size === 0) throw new Error('no pages published an og:image');
	return byImage;
}

async function capturePage(page, pageUrl) {
	await page.setViewport({ ...VIEWPORT, deviceScaleFactor: 1 });
	await page.goto(pageUrl, { waitUntil: 'load', timeout: 45000 });
	await page.evaluate(() => document.fonts.ready);
	await page.evaluate(prepareShareShot);
	const handle = (await page.$('#hero')) ?? (await page.$('main')) ?? (await page.$('body'));
	if (!handle) throw new Error(`no content to screenshot at ${pageUrl}`);
	await handle.evaluate((el) => el.scrollIntoView({ block: 'start', inline: 'start' }));
	const box = await handle.boundingBox();
	if (!box) throw new Error(`no box for ${pageUrl}`);
	const clip = clipBox(box, VIEWPORT);
	return page.screenshot({ type: 'png', clip });
}

function systemChrome() {
	if (process.platform === 'darwin') return '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
	if (process.platform === 'linux') return '/usr/bin/google-chrome';
	return null;
}

async function launchBrowser(puppeteer) {
	const args = ['--no-sandbox', '--disable-setuid-sandbox', '--disable-dev-shm-usage', '--allow-file-access-from-files'];
	const system = systemChrome();
	const bundled = { headless: true, args };
	const installed = system && existsSync(system) ? { headless: true, executablePath: system, args } : null;
	// On a Mac, Chrome for Testing in the Puppeteer cache is often incomplete.
	// The installed browser is the one that can take the screenshots.
	const attempts = process.platform === 'darwin' && installed ? [installed, bundled] : [bundled, installed].filter(Boolean);
	let last;
	for (const options of attempts) {
		try {
			return await puppeteer.launch(options);
		} catch (err) {
			last = err;
			const line = String(err.message || err).split('\n')[0];
			console.warn(line);
		}
	}
	throw last;
}

async function main() {
	if (!existsSync(distDir)) throw new Error('dist/ is missing — run astro build first');
	const version = releaseLabel(readSourceVersion(siteDir));
	const logo = readFileSync(path.join(siteDir, 'public', 'logo.png'));
	const logoUrl = `data:image/png;base64,${logo.toString('base64')}`;
	const pages = pagesToRender();
	const require = createRequire(import.meta.url);
	const puppeteer = require('puppeteer');
	const sharp = require('sharp');
	const { server, port } = await serve(distDir);
	const browser = await launchBrowser(puppeteer);
	try {
		const page = await browser.newPage();
		console.log(`share cards: ${version}, ${pages.size} pages`);
		for (const [image, info] of pages) {
			const pageUrl = `http://127.0.0.1:${port}${info.pathname}`;
			const shot = await capturePage(page, pageUrl);
			const shotUrl = `data:image/png;base64,${shot.toString('base64')}`;
			await page.setViewport({ width: 1200, height: 630, deviceScaleFactor: 1 });
			await page.setContent(
				cardHtml({
					version,
					kicker: kickerFor(info.pathname),
					title: info.title,
					description: info.description,
					shotUrl,
					logoUrl,
					shotPosition: info.pathname === '/' ? 'center top' : 'left top',
				}),
				{ waitUntil: 'domcontentloaded', timeout: 20000 },
			);
			await page.evaluate(
				() =>
					new Promise((resolve, reject) => {
						const img = document.querySelector('.frame img');
						if (!img) {
							reject(new Error('card image missing'));
							return;
						}
						if (img.complete && img.naturalWidth > 0) {
							resolve();
							return;
						}
						img.onload = () => resolve();
						img.onerror = () => reject(new Error('card image failed to load'));
					}),
			);
			const card = await page.screenshot({ type: 'png' });
			const out = distPathForOgUrl(image, distDir);
			mkdirSync(path.dirname(out), { recursive: true });
			await sharp(card).resize(1200, 630).png().toFile(out);
			const meta = await sharp(out).metadata();
			if (meta.width !== 1200 || meta.height !== 630) {
				throw new Error(`${out} is ${meta.width}x${meta.height}, expected 1200x630`);
			}
			console.log(`${info.pathname} -> ${path.relative(distDir, out)}`);
		}
	} finally {
		await browser.close();
		await new Promise((resolve) => server.close(resolve));
	}
}

main().catch((err) => {
	console.error(err.message || err);
	process.exit(1);
});
