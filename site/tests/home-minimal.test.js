import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { releaseAssets, getReleaseAsset, getDefaultArch } from '../src/scripts/minimal-download.ts';
const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = (p) => readFileSync(path.join(site, p), 'utf8');
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
    for (const pitch of ['26px 26px', '26.8px 26.8px', '25.4px 25.4px'])
        assert.ok(css.includes(pitch));
    for (const ink of ['#00ffff', '#ff00ff', '#ffff00'])
        assert.ok(css.includes(ink));
    assert.match(css, /mix-blend-mode:multiply/);
    assert.match(css, /\.ax-halftone-grid::after\{[^}]*color-mix\(in srgb,var\(--ax-lime\) 82%,transparent\)[^}]*mix-blend-mode:color/);
    assert.match(css, /mask-image:linear-gradient\(90deg,transparent 0%,transparent 18%,rgba\(0,0,0,\.22\) 36%,#000 58%,#000 100%\)/);
    assert.match(css, /\.ax-halftone-sharp\{[^}]*filter:blur\(\.4px\)/);
    assert.doesNotMatch(css, /infinite alternate/);
    assert.match(css, /animation:ax-moire-breath 24s ease-in-out infinite/);
    assert.match(css, /@keyframes ax-moire-breath\{0%,100%\{filter:blur\(4px\)\}50%\{filter:blur\(1\.2px\)\}\}/);
    assert.match(css, /animation:ax-moire-spin 240s linear infinite reverse/);
    assert.match(css, /animation:ax-moire-spin 320s linear infinite/);
    assert.match(css, /@keyframes ax-moire-spin\{to\{transform:rotate\(1turn\)\}\}/);
    assert.doesNotMatch(css, /translate3d/);
    assert.match(css, /prefers-reduced-motion:reduce\)\{html:not\(\[data-ax-motion='running'\]\)/);
    assert.match(css, /data-ax-motion='paused'\] \.ax-landing \.ax-halftone-grid\{animation-play-state:paused/);
    assert.match(home, /<MotionControl\s*\/>/);
    assert.match(css, /animation-delay:-11s/);
    assert.match(css, /animation-delay:-15\.5s/);
    assert.doesNotMatch(css, /border-radius:50%|ax-halftone-rosette/);
    assert.doesNotMatch(css, /@keyframes ax-moire-magenta\{0%,100%\{/);
    assert.doesNotMatch(css, /@keyframes ax-moire-yellow\{0%,100%\{/);
    assert.match(css, /\.ax-install-command\{[^}]*position:absolute[^}]*backdrop-filter:blur\(18px\)/);
    assert.doesNotMatch(css, /data-ax-install-panel\]:not\(\[hidden\]\).*overflow-y:auto/);
    assert.match(css, /@media\(max-width:600px\)\{[^\n]*\.ax-halftone\{inset:-18%;opacity:\.32\}/);
    assert.match(css, /prefers-reduced-motion:reduce/);
    assert.doesNotMatch(css, /background-image:\s*url\(/);
});
test('installer offers commands with manual OS selection and keeps optional release assets', () => {
    assert.match(home, /<MinimalDownload\s*\/>/);
    for (const icon of ['macOS', 'Linux', 'Windows'])
        assert.match(download, new RegExp(`platformIcons\\.${icon}`));
    assert.match(download, /AX_RELEASE_VERSION/);
    assert.match(download, /data-ax-platform-toggle/);
    assert.match(download, /data-ax-install-code/);
    assert.match(download, /data-ax-install-copy/);
    assert.match(download, /<button[^>]*data-ax-download/);
    assert.doesNotMatch(download, /releases\/latest\/download|Download for/);
    assert.match(read('src/scripts/minimal-download.ts'), /writeText\(installerCommand\(selected\)\)/);
    assert.equal(new Set(Object.values(releaseAssets).flatMap(assets => Object.values(assets))).size, 6);
    assert.equal(getReleaseAsset('macos', 'arm64'), 'https://github.com/GaryWenneker/ax/releases/latest/download/ax-darwin-arm64.tar.gz');
    assert.equal(getReleaseAsset('linux', 'x64'), 'https://github.com/GaryWenneker/ax/releases/latest/download/ax-linux-x64.tar.gz');
    assert.equal(getReleaseAsset('windows', 'arm64'), 'https://github.com/GaryWenneker/ax/releases/latest/download/ax-win32-arm64.zip');
    assert.equal(getReleaseAsset('wsl2', 'x64'), getReleaseAsset('linux', 'x64'));
    assert.equal(getDefaultArch('macos'), 'arm64');
    assert.equal(getDefaultArch('windows'), 'x64');
});
test('phone landing fits the viewport and the platform menu overlays with a blur', () => {
    const phone = css.split('\n').find((line) => line.includes('@media(max-width:600px)') && line.includes('.ax-main'));
    assert.ok(phone, 'missing phone layout');
    assert.match(phone, /\.ax-main\{[^}]*padding:8px 20px 4px/);
    assert.match(phone, /\.ax-display-word\{[^}]*font-size:48px/);
    assert.match(phone, /\.ax-platform-panel\{[^}]*position:absolute[^}]*bottom:calc\(100% \+ 8px\)/);
    assert.doesNotMatch(phone, /position:relative;bottom:auto/);
    assert.match(css, /\.ax-platform-panel\{[^}]*backdrop-filter:blur\(18px\)/);
    assert.match(css, /\.ax-platform-panel\{[^}]*background:rgba\(18,23,19,\.62\)/);
});
//# sourceMappingURL=home-minimal.test.js.map