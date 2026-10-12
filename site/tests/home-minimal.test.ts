import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { releaseAssets, getReleaseAsset, getDefaultArch } from '../src/scripts/minimal-download.ts';

const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = (pathWithinSite: string): string => readFileSync(path.join(site, pathWithinSite), 'utf8');
const home = read('src/components/MinimalHome.astro');
const download = read('src/components/MinimalDownload.astro');
const css = read('src/styles/minimal-home.css');
const index = read('src/pages/index.astro');

test('homepage describes Ax without marketing clutter', () => {
  for (const term of ['Your codebase', 'understood.', 'Code intelligence', 'Persistent memory', 'MCP', 'Quality gates', 'Rust']) {
    assert.ok(home.includes(term), `missing ${term}`);
  }
  assert.match(home, /<MinimalDownload\s*\/>/);
  assert.doesNotMatch(home, /VectorLandscape|AresOverlay|LiveTerminal|ScrollReveal/);
  assert.doesNotMatch(index, /home-cinematic|landscape-svg|ares-identity|live-terminal|ScrollReveal/);
});

test('Oxanium technical display, black and one lime accent', () => {
  assert.match(index, /Oxanium/);
  assert.match(css, /font-family:'Oxanium'/);
  assert.match(css, /--ax-lime:#b8fc7a/);
  assert.match(css, /@media\(min-width:900px\)/);
  assert.match(css, /overflow:hidden/);
});

test('moire is a layered, moving print-dot grid, not an image', () => {
  assert.match(home, /ax-moire-layer one/);
  assert.match(home, /ax-moire-layer two/);
  assert.match(css, /background-image:radial-gradient/);
  assert.match(css, /7\.35px 7\.35px/);
  assert.match(css, /@keyframes ax-moire-interference/);
  assert.match(css, /prefers-reduced-motion:reduce/);
  assert.doesNotMatch(css, /url\(|<canvas|<image/);
});

test('download UI has OS icons, fallback architecture choice and honest source version', () => {
  assert.match(download, /platformIcons\.macOS/);
  assert.match(download, /platformIcons\.Linux/);
  assert.match(download, /platformIcons\.Windows/);
  assert.match(download, /AX_RELEASE_VERSION/);
  assert.match(download, /data-ax-platform-toggle/);
  assert.match(download, /data-ax-arch="arm64"/);
  assert.match(download, /data-ax-arch="x64"/);
});

test('all six downloadable native release assets map correctly', () => {
  assert.equal(new Set(Object.values(releaseAssets).flatMap(m => Object.values(m))).size, 6);
  assert.equal(getReleaseAsset('macos','arm64'), 'https://github.com/GaryWenneker/ax/releases/latest/download/ax-darwin-arm64.tar.gz');
  assert.equal(getReleaseAsset('linux','x64'), 'https://github.com/GaryWenneker/ax/releases/latest/download/ax-linux-x64.tar.gz');
  assert.equal(getReleaseAsset('windows','arm64'), 'https://github.com/GaryWenneker/ax/releases/latest/download/ax-win32-arm64.zip');
  assert.equal(getReleaseAsset('wsl2','x64'), getReleaseAsset('linux','x64'));
  assert.equal(getDefaultArch('macos'), 'arm64');
  assert.equal(getDefaultArch('windows'), 'x64');
});