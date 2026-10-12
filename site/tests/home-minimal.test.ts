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

test('CMY interference fades horizontally from the edge and uses continuous tile loops', () => {
  assert.match(home, /ax-halftone-screen-three/);
  for (const pitch of ['26px 26px', '26.8px 26.8px', '25.4px 25.4px']) assert.ok(css.includes(pitch));
  for (const ink of ['#00ffff', '#ff00ff', '#ffff00']) assert.ok(css.includes(ink));
  assert.match(css, /mix-blend-mode:multiply/);
  assert.match(css, /\.ax-halftone-grid::after\{[^}]*var\(--ax-lime\)[^}]*mix-blend-mode:color/);
  assert.match(css, /mask-image:linear-gradient\(90deg,transparent 0%/);
  assert.match(css, /#000 100%/);
  assert.doesNotMatch(css, /mask-image:radial-gradient|infinite alternate/);
  assert.match(css, /animation:ax-moire-magenta 72s linear infinite/);
  assert.match(css, /animation:ax-moire-yellow 96s linear infinite/);
  assert.match(css, /animation:ax-moire-focus 64s ease-in-out infinite/);
  assert.match(css, /@keyframes ax-moire-focus\{0%,43%,70%,100%\{filter:blur\(0\)\}53%,59%\{filter:blur\(1\.5px\)\}\}/);
  assert.match(css, /prefers-reduced-motion:reduce\)\{\.ax-landing \.ax-halftone-dots,\.ax-landing \.ax-halftone-grid\{animation:none!important;filter:none!important/);
  assert.match(css, /data-ax-motion='paused'\] \.ax-landing \.ax-halftone-grid\{animation-play-state:paused!important/);
  assert.doesNotMatch(css, /border-radius:50%|ax-halftone-rosette/);
  assert.match(css, /@keyframes ax-moire-magenta/);
  assert.match(css, /@keyframes ax-moire-yellow/);
  assert.match(css, /prefers-reduced-motion:reduce/);
  assert.doesNotMatch(css, /background-image:\s*url\(/);
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
