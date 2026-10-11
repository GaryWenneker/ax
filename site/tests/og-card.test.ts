import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
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
	shareTitle,
} from '../src/lib/og-card.mjs';

const siteDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

test('each page gets its own share-image path', () => {
	assert.equal(ogImagePath('/'), '/og/home.png');
	assert.equal(ogImagePath('/guides/policy-engine/'), '/og/guides/policy-engine.png');
	assert.equal(ogImagePath('/getting-started/introduction'), '/og/getting-started/introduction.png');
	assert.equal(ogImagePath('/guides/../secret'), '/og/home.png');
	assert.equal(ogImagePath('https://evil.example/x'), '/og/home.png');
});

test('kickers follow the section, and the home page is the current version', () => {
	assert.equal(kickerFor('/'), 'Current version');
	assert.equal(kickerFor('/guides/policy-engine/'), 'Guide');
	assert.equal(kickerFor('/reference/cli/'), 'Reference');
	assert.equal(kickerFor('/core-concepts/how-it-works/'), 'Core concepts');
	assert.equal(kickerFor('/getting-started/quickstart/'), 'Getting started');
});

test('share titles drop the site suffix and the ax prefix', () => {
	assert.equal(shareTitle('Policy Engine | ax'), 'Policy Engine');
	assert.equal(shareTitle('ax — Graph it. Remember it. Ship it.'), 'Graph it. Remember it. Ship it.');
	assert.equal(shareTitle(''), 'ax');
});

test('release labels must look like v7.0.0', () => {
	assert.equal(releaseLabel('v7.0.0\n'), 'v7.0.0');
	assert.match(releaseLabel(readFileSync(path.join(siteDir, 'public', 'releases', 'latest.txt'), 'utf8')), /^v\d+\.\d+\.\d+$/);
	assert.throws(() => releaseLabel('5.0.0'), /v7\.0\.0/);
	assert.throws(() => releaseLabel(''), /v7\.0\.0/);
});

test('the card paints the given release and escapes the page title', () => {
	const html = cardHtml({
		version: 'v7.0.0',
		kicker: 'Guide',
		title: 'Policy <Engine>',
		description: 'Rules & skills',
		shotUrl: 'file:///shot.png',
		logoUrl: 'file:///logo.png',
	});
	assert.match(html, /v7\.0\.0/);
	assert.doesNotMatch(html, /5\.0\.0/);
	assert.match(html, /Policy &lt;Engine&gt;/);
	assert.match(html, /Rules &amp; skills/);
	assert.doesNotMatch(html, /Policy <Engine>/);
	assert.match(html, /1200/);
	assert.match(html, /630/);
	assert.match(html, /object-position:left top/);
});

test('share meta and dist paths stay inside og/', () => {
	const html = `<title>Ignored</title>
		<meta property="og:title" content="Policy Engine | ax" />
		<meta name="description" content="How policy works" />
		<meta property="og:image" content="https://getax.wenneker.io/og/guides/policy-engine.png" />`;
	assert.deepEqual(readShareMeta(html), {
		image: 'https://getax.wenneker.io/og/guides/policy-engine.png',
		title: 'Policy Engine | ax',
		description: 'How policy works',
	});
	const dist = path.join(siteDir, 'dist');
	assert.equal(
		distPathForOgUrl('https://getax.wenneker.io/og/guides/policy-engine.png', dist),
		path.join(dist, 'og', 'guides', 'policy-engine.png'),
	);
	assert.throws(() => distPathForOgUrl('https://getax.wenneker.io/social/v5.0.0/ax.png', dist), /og/);
	assert.throws(() => distPathForOgUrl('https://evil.example/og/home.png', dist), /origin/);
});

test('a built index.html maps to the home path', () => {
	const dist = path.join(siteDir, 'dist');
	assert.equal(pathnameFromDistHtml(dist, path.join(dist, 'index.html')), '/');
	assert.equal(
		pathnameFromDistHtml(dist, path.join(dist, 'guides', 'policy-engine', 'index.html')),
		'/guides/policy-engine/',
	);
	assert.equal(pathnameFromDistHtml(dist, path.join(dist, '404.html')), '/404');
});

test('the share shot clips to the viewport and keeps the top of the page', () => {
	assert.deepEqual(clipBox({ x: 0, y: 80, width: 2000, height: 4000 }, { width: 1000, height: 800 }), {
		x: 0,
		y: 80,
		width: 1000,
		height: 640,
	});
});

test('the shot hides docs chrome and forces the landing hero visible', () => {
	const src = prepareShareShot.toString();
	assert.match(src, /nav\.sidebar/);
	assert.match(src, /right-sidebar-container/);
	assert.match(src, /header\.nav/);
	assert.match(src, /revealed/);
	assert.match(src, /opacity:\s*1/);
});

test('the site no longer points every page at the v5 card', () => {
	const config = readFileSync(path.join(siteDir, 'astro.config.mjs'), 'utf8');
	const home = readFileSync(path.join(siteDir, 'src', 'pages', 'index.astro'), 'utf8');
	assert.doesNotMatch(config, /v5\.0\.0/);
	assert.doesNotMatch(home, /v5\.0\.0/);
	assert.match(home, /ogImagePath/);
	assert.match(config, /Head\.astro/);
});
