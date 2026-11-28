/**
 * Render every raster and copied logo asset from the SVG masters in assets/:
 * - logo.svg        dark mark, for light backgrounds
 * - logo-light.svg  white mark, for dark backgrounds
 * - logo-icon.svg   app icon: the white mark on a dark rounded square
 *
 * Run from site/: node scripts/process-logo.mjs
 */
import sharp from 'sharp';
import { copyFileSync, existsSync, writeFileSync } from 'fs';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const repo = join(dirname(fileURLToPath(import.meta.url)), '../..');
const at = (...parts) => join(repo, ...parts);

const ICON_BACKGROUND = '#080A09';
const MARK_WIDTH = 1296;
const MARK_HEIGHT = 800;

const mark = at('assets/logo.svg');
const markLight = at('assets/logo-light.svg');
const icon = at('assets/logo-icon.svg');
const webUi = at('crates/ax-web/web-ui');

/** The mark on a transparent canvas, cropped to its own bounds. */
function markPng(source, output) {
	return sharp(source, { density: 600 }).resize(MARK_WIDTH, MARK_HEIGHT).png().toFile(output);
}

/** The app icon as an opaque square of the given size. */
function iconPng(size, output) {
	return sharp(icon, { density: 600 })
		.resize(size, size)
		.flatten({ background: ICON_BACKGROUND })
		.png()
		.toFile(output);
}

await markPng(mark, at('assets/logo.png'));
await markPng(markLight, at('site/public/logo.png'));

// Source art on white, the way the mark is presented on its own.
const centered = await sharp(mark, { density: 600 }).resize(960).png().toBuffer();
await sharp({ create: { width: 1600, height: 1600, channels: 3, background: '#FFFFFF' } })
	.composite([{ input: centered, gravity: 'centre' }])
	.jpeg({ quality: 92 })
	.toFile(at('assets/logo.jpg'));

await sharp(markLight, { density: 600 })
	.resize(128, 128, { fit: 'contain', background: { r: 0, g: 0, b: 0, alpha: 0 } })
	.png()
	.toFile(join(webUi, 'public/ax-mark.png'));
await iconPng(256, join(webUi, 'public/ax-logo.png'));
await iconPng(192, join(webUi, 'public/icon-192.png'));
await iconPng(512, join(webUi, 'public/icon-512.png'));

const mcpIcon = at('crates/ax-mcp/assets/ax-icon.png');
await iconPng(128, mcpIcon);
// The MCP server inlines this file into a data URI, so it carries no newline.
writeFileSync(`${mcpIcon}.b64`, (await sharp(mcpIcon).toBuffer()).toString('base64'));

for (const target of [
	'site/public/favicon.svg',
	'crates/ax-admin/web-ui/public/icon.svg',
	'crates/ax-web/web-ui/public/favicon.svg',
]) {
	// The admin UI's public folder only exists once that app ships its own icon.
	if (existsSync(dirname(at(target)))) copyFileSync(icon, at(target));
}

// The Command Center build is committed; keep its static copies current.
if (existsSync(join(webUi, 'dist'))) {
	for (const name of ['ax-logo.png', 'ax-mark.png', 'icon-192.png', 'icon-512.png', 'favicon.svg']) {
		copyFileSync(join(webUi, 'public', name), join(webUi, 'dist', name));
	}
}

console.log('Logo assets written from assets/logo*.svg');
