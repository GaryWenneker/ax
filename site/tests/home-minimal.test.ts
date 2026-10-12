import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { releaseAssets, getReleaseAsset, getDefaultArch } from '../src/scripts/minimal-download.ts';

const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = (p: string) => readFileSync(path.join(site, p), 'utf8');
const home = read('src/components/MinimalHome.astro');
const css = read('src/styles/minimal-home.css');
const page = read('src/pages/index.astro');
const download = read('src/components/MinimalDownload.astro');
const pkg = JSON.parse(read('package.json'));

// The approved copy, colors, and typography must stay in sync between
// the visible homepage, metadata, locally bundled font, and responsive rules.
test('approved three-line slogan, only control receives lime accent', () => {
  assert.match(home, /<h1 class="ax-headline">\s*<span>Your context\.<\/span><span>Your rules\.<\/span><span class="ax-headline-accent">Your control\.<\/span><\/h1>/);
  assert.match(css, /\.ax-headline \.ax-headline-accent\s*\{color:var\(--ax-lime\)\}/);
  assert.doesNotMatch(home, /Your codebase|understood\.|Code context kept/);
  assert.match(page, /Your context\. Your rules\. Your control\./);
});

test('Oxanium is bundled and used for the heading, not loaded from Google Fonts', () => {
  assert.equal(pkg.dependencies['@fontsource-variable/oxanium'], '^5.3.0');
  assert.match(page, /@fontsource-variable\/oxanium\/wght\.css/);
  assert.match(css, /\.ax-headline\{[^}]*font-family:'Oxanium Variable'/);
  assert.doesNotMatch(page, /fonts\.googleapis\.com|fonts\.gstatic\.com/);
});

test('pop-art lime moire grid remains CSS-only, moving and accessible', () => {
  assert.match(home, /ax-moire-layer one/);
  assert.match(home, /ax-moire-layer two/);
  assert.match(css, /17px 17px/);
  assert.match(css, /18\.8px 18\.8px/);
  assert.match(css, /rgba\(185,255,106/);
  assert.match(css, /@keyframes ax-moire-interference/);
  assert.match(css, /prefers-reduced-motion:reduce/);
  assert.match(css, /@media\(min-width:900px\)/);
  assert.doesNotMatch(css, /background-image:\s*url\(/);
  assert.doesNotMatch(home, /VectorLandscape|AresOverlay|LiveTerminal|ScrollReveal/);
});

test('installer offers the six real release assets and manual OS and CPU selection', () => {
  assert.match(home, /<MinimalDownload\s*\/>/);
  for (const icon of ['macOS', 'Linux', 'Windows']) assert.match(download, new RegExp(`platformIcons\\.${icon}`));
  assert.match(download, /AX_RELEASE_VERSION/);
  assert.match(download, /data-ax-platform-toggle/);
  assert.match(download, /data-ax-arch="arm64"/);
  assert.match(download, /data-ax-arch="x64"/);
  assert.equal(new Set(Object.values(releaseAssets).flatMap(assets => Object.values(assets))).size, 6);
  assert.equal(getReleaseAsset('macos', 'arm64'), 'https://github.com/GaryWenneker/ax/releases/latest/download/ax-darwin-arm64.tar.gz');
  assert.equal(getReleaseAsset('linux', 'x64'), 'https://github.com/GaryWenneker/ax/releases/latest/download/ax-linux-x64.tar.gz');
  assert.equal(getReleaseAsset('windows', 'arm64'), 'https://github.com/GaryWenneker/ax/releases/latest/download/ax-win32-arm64.zip');
  assert.equal(getReleaseAsset('wsl2', 'x64'), getReleaseAsset('linux', 'x64'));
  assert.equal(getDefaultArch('macos'), 'arm64');
  assert.equal(getDefaultArch('windows'), 'x64');
});
