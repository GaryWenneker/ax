import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { existsSync } from 'node:fs';
import {
  releaseAssets, getReleaseAsset, getDefaultArch, mountMinimalDownload,
  installProducts, installerCommand, platformFor, type Product,
} from '../src/scripts/minimal-download.ts';
import { allDecls, decl, normalizeValue, parseCss, reformat, rulesFor, type Sheet } from './css-model.ts';

const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = (p: string) => readFileSync(path.join(site, p), 'utf8');
const home = read('src/components/MinimalHome.astro');
const cssSource = read('src/styles/minimal-home.css');
const css = parseCss(cssSource);
// Every CSS check runs on the file as written and on a formatter's version of
// it, so the tests follow what the CSS means rather than its exact text.
const sheets: [string, Sheet][] = [['as written', css], ['reformatted', parseCss(reformat(cssSource))]];
const L = '.ax-landing ';
const same = (actual: string | undefined, expected: string, what: string) =>
  assert.equal(actual, normalizeValue(expected), what);
const page = read('src/pages/index.astro');
const download = read('src/components/MinimalDownload.astro');
const pkg = JSON.parse(read('package.json'));

// The approved copy, colors, and typography must stay in sync between
// the visible homepage, metadata, locally bundled font, and responsive rules.
test('approved three-line slogan, only control receives lime accent', () => {
  assert.match(home, /<h1 class="ax-headline">\s*<span>Your context\.<\/span><span>Your rules\.<\/span><span class="ax-headline-accent">Your control\.<\/span><\/h1>/);
  for (const [label, sheet] of sheets) same(decl(sheet, `${L}.ax-headline .ax-headline-accent`, 'color'), 'var(--ax-lime)', label);
  assert.doesNotMatch(home, /Your codebase|understood\.|Code context kept/);
  assert.match(page, /Your context\. Your rules\. Your control\./);
});

test('Oxanium is bundled and used for the heading, not loaded from Google Fonts', () => {
  assert.equal(pkg.dependencies['@fontsource-variable/oxanium'], '^5.3.0');
  assert.match(page, /@fontsource-variable\/oxanium\/wght\.css/);
  for (const [label, sheet] of sheets) assert.match(decl(sheet, `${L}.ax-headline`, 'font-family') ?? '', /^'Oxanium Variable'/, label);
  assert.doesNotMatch(page, /fonts\.googleapis\.com|fonts\.gstatic\.com/);
});

test('homepage backdrop is the deck backdrop (halftone, silk ring, vignette), not the CMY moiré', () => {
  const backdrop = read('src/components/DeckBackdrop.astro');
  const script = read('src/scripts/deck-backdrop.ts');
  const stylesSource = read('src/styles/deck-backdrop.css');
  assert.match(home, /<DeckBackdrop\s*\/>/);
  assert.doesNotMatch(home + cssSource, /ax-halftone|ax-moire/);
  assert.match(backdrop, /data-ax-backdrop-root data-ax-variant=\{variant\} aria-hidden="true"/);
  for (const hook of ['data-ax-backdrop-dots', 'data-ax-silk-fog', 'data-ax-silk', 'ax-backdrop-vignette']) assert.ok(backdrop.includes(hook), hook);
  assert.match(backdrop, /mountDeckBackdrop\(root\)/);
  assert.match(backdrop, /astro:before-swap/);
  for (const ink of ['#6d28d9', '#7c3aed', '#a78bfa', '#d946ef', '#c4b5fd', '#b9ff6a']) assert.ok(script.includes(ink), ink);
  // Motion: the visible Pause motion control and hidden tabs stop the loop on a still frame.
  assert.match(script, /html\.dataset\.axMotion === 'paused' \|\| document\.hidden/);
  assert.match(script, /attributeFilter: \['data-theme', 'data-ax-motion'\]/);
  assert.match(script, /addEventListener\('resize', onResize\)/);
  assert.match(script, /addEventListener\('pagehide', save\)/);
  assert.match(home, /<MotionControl\s*\/>/);
  const backdropSheets: [string, Sheet][] = [['as written', parseCss(stylesSource)], ['reformatted', parseCss(reformat(stylesSource))]];
  for (const [label, styles] of backdropSheets) {
    // The deck's vignette keeps the copy readable; the light docs theme hides the glow.
    assert.ok((decl(styles, '.ax-backdrop-vignette', 'background') ?? '').startsWith(
      normalizeValue('radial-gradient(ellipse at 30% 50%,rgba(8,10,9,.66)')), `${label}: vignette`);
    assert.deepEqual(['position', 'z-index', 'inset', 'pointer-events'].map((p) => decl(styles, '.ax-backdrop', p)),
      ['fixed', '0', '0', 'none'], `${label}: fixed backdrop`);
    // Nothing over the animated canvases forces a per-frame recomposite.
    for (const d of allDecls(styles)) {
      assert.ok(!['mix-blend-mode', 'filter', 'backdrop-filter', '-webkit-backdrop-filter'].includes(d.prop), `${label}: ${d.prop} in ${d.selector}`);
    }
    same(decl(styles, ":root[data-theme='light'] .ax-backdrop", 'display'), 'none', `${label}: light theme hides the backdrop`);
  }
  for (const [label, sheet] of sheets) {
    const at = (selector: string, prop: string, media = '') => decl(sheet, `${L}${selector}`, prop, media);
    // Deck kicker and lime chips on the hero.
    same(at('.ax-eyebrow', 'color'), 'var(--ax-lime)', `${label}: eyebrow`);
    same(at('.ax-feature-strip>*', 'border-radius'), '99em', `${label}: feature chips`);
    same(at('.ax-install-command', 'position'), 'absolute', `${label}: install command`);
    same(at('.ax-install-command', 'backdrop-filter'), 'blur(18px)', `${label}: install command blur`);
    assert.ok(allDecls(sheet).some((d) => d.media.includes('prefers-reduced-motion:reduce')), `${label}: reduced motion rules`);
    for (const d of allDecls(sheet)) {
      assert.ok(!(d.prop.startsWith('background') && /url\(/.test(d.value)), `${label}: no images in ${d.selector}`);
      if (/data-ax-install-panel\]:not\(\[hidden\]\)/.test(d.selector)) assert.notEqual(d.prop, 'overflow-y', label);
    }
  }
});

test('backdrop layout: hero clears the copy, docs pages each get their own stable composition', async () => {
  const { backdropLayout, makeSparks, pageSeed } = await import('../src/scripts/deck-backdrop.ts');
  assert.deepEqual(backdropLayout('hero', 375, 812), { fx: 0.92, fy: 0.16, r: 0.5, k: 1, spin: 0 });
  assert.deepEqual(backdropLayout('hero', 768, 1024), { fx: 0.9, fy: 0.3, r: 0.44, k: 1, spin: 0 });
  const desktop = backdropLayout('hero', 1440, 900);
  assert.equal(desktop.fx, 0.76);
  assert.ok(desktop.r <= 0.4);
  assert.equal(pageSeed('/guides/memory/'), pageSeed('/guides/memory'));
  assert.notEqual(pageSeed('/guides/memory/'), pageSeed('/guides/policy-engine/'));
  for (const path of ['/getting-started/introduction/', '/guides/memory/', '/reference/cli/']) {
    const page = backdropLayout('page', 1440, 900, pageSeed(path));
    assert.equal(page.k, 0.45);
    assert.ok(page.fx >= 0.58 && page.fx <= 0.9, `${path} ring stays right of the article (${page.fx})`);
  }
  assert.deepEqual(makeSparks(5), makeSparks(5));
});

test('the halftone grid is a fixed page background, independent of the ring', async () => {
  const { gridWeight, backdropLayout, pageSeed } = await import('../src/scripts/deck-backdrop.ts');
  // Same weight field on every page: it depends on the viewport only, never on the ring layout.
  assert.equal(gridWeight.length, 3);
  const hero = backdropLayout('hero', 1440, 900);
  const docs = backdropLayout('page', 1440, 900, pageSeed('/guides/memory/'));
  assert.notDeepEqual([hero.fx, hero.fy], [docs.fx, docs.fy]);
  assert.ok(gridWeight(0.9, 0.45, 1.6) > 0.8, 'strong at the right edge');
  assert.ok(gridWeight(0.1, 0.45, 1.6) < 0, 'calm behind the copy on the left');
  // Phones: the grid gathers top-right and stays out from behind the full-width copy.
  assert.ok(gridWeight(0.95, 0.08, 0.46) > 0.9);
  assert.ok(gridWeight(0.3, 0.45, 0.46) < -0.5, 'at most a few faint dots behind the phone copy at wave peaks');
});

test('page changes carry the ring state so the ring travels like a deck slide change', async () => {
  const { readSavedState, RIBBONS } = await import('../src/scripts/deck-backdrop.ts');
  const bodies = RIBBONS.map(() => ({ fx: 0.76, fy: 0.5, vx: 0, vy: 0 }));
  const state = { at: 1_000_000, clock: 12.5, r: 0.4, k: 1, spin: 0, bodies };
  assert.deepEqual(readSavedState(JSON.stringify(state), 1_002_000), state);
  assert.equal(readSavedState(JSON.stringify(state), 1_000_000 + 16_000), null, 'stale state is ignored');
  assert.equal(readSavedState(JSON.stringify({ ...state, bodies: bodies.slice(1) }), 1_001_000), null);
  assert.equal(readSavedState(JSON.stringify({ ...state, clock: 'x' }), 1_001_000), null);
  assert.equal(readSavedState('{broken', 1_001_000), null);
  assert.equal(readSavedState(null, 1_001_000), null);
  // Layers follow on their own springs: mist trails, the bright core leads.
  const stiffness = RIBBONS.map((ribbon) => ribbon.K);
  assert.deepEqual([...stiffness].sort((x, y) => x - y), stiffness);
});

test('phones, small screens, weak CPUs and Windows start at a lighter quality tier', async () => {
  const { startTier, TIERS } = await import('../src/scripts/deck-backdrop.ts');
  const mac = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)';
  assert.equal(startTier({ ua: mac, cores: 10, memory: 8, coarse: false, width: 1440 }), 0);
  assert.equal(startTier({ ua: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)', cores: 12, memory: 8, coarse: false, width: 1920 }), 1);
  assert.equal(startTier({ ua: mac, cores: 8, memory: 4, coarse: true, width: 390 }), 1);
  assert.equal(startTier({ ua: mac, cores: 4, memory: 8, coarse: false, width: 1440 }), 2);
  assert.equal(startTier({ ua: mac, cores: 8, memory: 2, coarse: true, width: 390 }), 3);
  assert.equal(TIERS.at(-1)?.every, 2, 'the lowest tier paces to a steady 30 fps');
});

test('the ring stays sharp at rest: never below screen resolution, Retina-sharp when affordable', async () => {
  const { canvasScale, TIERS } = await import('../src/scripts/deck-backdrop.ts');
  for (let tier = 0; tier < TIERS.length; tier++) {
    for (const [w, h, dpr] of [[1440, 900, 2], [2560, 1440, 2], [3840, 2160, 1], [390, 844, 3], [1024, 768, 1]]) {
      assert.ok(canvasScale(tier, dpr, w, h) >= 1, `tier ${tier} ${w}x${h}@${dpr}`);
    }
  }
  assert.equal(canvasScale(0, 2, 1440, 900), 2, 'full Retina on a laptop screen');
  assert.equal(canvasScale(1, 3, 390, 844), 1.5, 'phones draw at 1.5x');
  assert.equal(canvasScale(0, 1, 1920, 1080), 1, 'never above the device pixel ratio');
});

test('the quality monitor steps down on slow frames and back up when frames are fast', async () => {
  const { judgeFrames } = await import('../src/scripts/deck-backdrop.ts');
  const run = (frame: number, count: number, tier: number, tooSlow = new Set<number>()) => {
    const m = { n: 0, sum: 0, spikes: 0 };
    let t = tier;
    for (let i = 0; i < count; i++) {
      const next = judgeFrames(m, frame, t, tooSlow);
      if (next !== t) return next;
    }
    return t;
  };
  assert.equal(run(1 / 60, 200, 0), 0, '60 fps keeps the top tier');
  assert.equal(run(1 / 40, 30, 0), 1, 'a slow average steps down after 30 frames');
  assert.equal(run(1 / 120, 90, 2), 1, 'fast frames step back up after 90');
  assert.equal(run(1 / 120, 200, 2, new Set([1])), 2, 'never back into a tier that was too slow');
  const spiky = { n: 0, sum: 0, spikes: 0 };
  let tier = 0;
  for (let i = 0; i < 30 && tier === 0; i++) tier = judgeFrames(spiky, i % 6 === 0 ? 0.034 : 0.012, tier, new Set());
  assert.equal(tier, 1, 'repeated dropped frames step down even when the average looks fine');
});

test('installer offers commands with manual OS selection and keeps optional release assets', () => {
  assert.match(home, /<MinimalDownload\s*\/>/);
  for (const icon of ['macOS', 'Linux', 'Windows']) assert.match(download, new RegExp(`platformIcons\\.${icon}`));
  assert.match(download, /AX_RELEASE_VERSION/);
  assert.match(download, /data-ax-platform-toggle/);
  assert.match(download, /data-ax-install-code/);
  assert.match(download, /data-ax-install-copy/);
  assert.match(download, /<button[^>]*data-ax-download/);
  assert.doesNotMatch(download, /releases\/latest\/download|Download for/);
  assert.match(read('src/scripts/minimal-download.ts'), /const command = installerCommand\(selected, chosen\);[\s\S]*writeText\(command\)/);
  assert.equal(new Set(Object.values(releaseAssets).flatMap(assets => Object.values(assets))).size, 6);
  assert.equal(getReleaseAsset('macos', 'arm64'), 'https://github.com/GaryWenneker/ax/releases/latest/download/ax-darwin-arm64.tar.gz');
  assert.equal(getReleaseAsset('linux', 'x64'), 'https://github.com/GaryWenneker/ax/releases/latest/download/ax-linux-x64.tar.gz');
  assert.equal(getReleaseAsset('windows', 'arm64'), 'https://github.com/GaryWenneker/ax/releases/latest/download/ax-win32-arm64.zip');
  assert.equal(getReleaseAsset('wsl2', 'x64'), getReleaseAsset('linux', 'x64'));
  assert.equal(getDefaultArch('macos'), 'arm64');
  assert.equal(getDefaultArch('windows'), 'x64');
});

test('homepage installer offers Ax, headless Linux and the admin app with their own commands', () => {
  assert.deepEqual(Object.keys(installProducts), ['ax', 'slim', 'admin']);
  assert.deepEqual(installProducts.ax.platforms, ['macos', 'linux', 'windows', 'wsl2']);
  assert.deepEqual(installProducts.slim.platforms, ['linux']);
  assert.deepEqual(installProducts.admin.platforms, ['macos', 'linux', 'windows']);
  assert.equal(installerCommand('macos'), 'curl -fsSL https://getax.wenneker.io/install.sh | sh');
  assert.equal(installerCommand('windows', 'ax'), 'irm https://getax.wenneker.io/install.ps1 | iex');
  assert.equal(installerCommand('linux', 'slim'), 'curl -fsSL https://getax.wenneker.io/install-slim.sh | sh');
  assert.equal(installerCommand('macos', 'admin'), 'curl -fsSL https://getax.wenneker.io/install-admin.sh | sh');
  assert.equal(installerCommand('linux', 'admin'), 'curl -fsSL https://getax.wenneker.io/install-admin.sh | sh');
  assert.equal(installerCommand('windows', 'admin'), 'irm https://getax.wenneker.io/install-admin.ps1 | iex');
  // Every command points at an installer the site actually serves.
  for (const product of Object.keys(installProducts) as Product[]) {
    for (const os of installProducts[product].platforms) {
      const file = installerCommand(os, product).match(/getax\.wenneker\.io\/([\w.-]+)/)?.[1];
      assert.ok(file && existsSync(path.join(site, 'public', file)), `${product}/${os}: ${file}`);
    }
  }
  // A platform the product does not support moves to one it does.
  assert.equal(platformFor('slim', 'macos'), 'linux');
  assert.equal(platformFor('slim', 'windows'), 'linux');
  assert.equal(platformFor('admin', 'wsl2'), 'linux');
  assert.equal(platformFor('admin', 'windows'), 'windows');
  assert.equal(platformFor('ax', 'wsl2'), 'wsl2');
  assert.equal(platformFor('slim', null), 'linux');
  assert.equal(platformFor('ax', null), null);
  assert.equal(installProducts.ax.guide, '/getting-started/installation/');
  assert.equal(installProducts.slim.guide, '/guides/headless-linux/');
  assert.equal(installProducts.admin.guide, '/guides/remote-admin/');
  for (const product of ['ax', 'slim', 'admin']) {
    assert.match(download, new RegExp(`data-ax-product=\\{?["']?${product}|id:\\s*'${product}'`));
  }
  assert.match(download, /data-ax-product=/);
  assert.match(download, /data-ax-guide/);
  // Without the extension Vite bundles the stale compiled minimal-download.js next to the source.
  assert.match(download, /import \{ mountMinimalDownload \} from '\.\.\/scripts\/minimal-download\.ts';/);
  for (const [label, sheet] of sheets) same(decl(sheet, '.ax-platform-options button[hidden]', 'display'), 'none !important', label);
});

test('choosing a product limits the platforms, keeps the menu open and copies its own command', async () => {
  const copied: string[] = [];
  type El = {
    sel: string; dataset: Record<string, string>; hidden: boolean; textContent: string; innerHTML: string;
    href: string; attrs: Record<string, string>; kids: El[]; on: Map<string, (() => void)[]>;
    classList: { add: () => void; remove: () => void; contains: () => boolean };
    setAttribute: (n: string, v: string) => void; getAttribute: (n: string) => string | undefined;
    addEventListener: (t: string, fn: () => void) => void; contains: (n: unknown) => boolean;
    querySelector: (s: string) => El | null; querySelectorAll: (s: string) => El[];
  };
  function el(sel: string, dataset: Record<string, string> = {}): El {
    const e: El = {
      sel, dataset, hidden: sel === '[data-ax-install-panel]' || sel === '[data-ax-platform-panel]',
      textContent: '', innerHTML: '', href: '', attrs: {}, kids: [], on: new Map(),
      classList: { add() {}, remove() {}, contains: () => false },
      setAttribute(n, v) { e.attrs[n] = v; },
      getAttribute(n) { return e.attrs[n]; },
      addEventListener(t, fn) { e.on.set(t, [...(e.on.get(t) ?? []), fn]); },
      contains(n) { return n === e || e.kids.some(k => k.contains(n)); },
      querySelector(s) { return e.querySelectorAll(s)[0] ?? null; },
      querySelectorAll(s) {
        const out: El[] = [];
        const visit = (k: El) => { if (k.sel === s) out.push(k); k.kids.forEach(visit); };
        e.kids.forEach(visit);
        return out;
      },
    };
    return e;
  }
  const root = el('[data-ax-downloader]');
  const parts = Object.fromEntries([
    'data-ax-download', 'data-ax-download-label', 'data-ax-main-icon', 'data-ax-platform-label',
    'data-ax-platform-toggle', 'data-ax-platform-panel', 'data-ax-platform-help', 'data-ax-install-panel',
    'data-ax-install-code', 'data-ax-install-instruction', 'data-ax-install-copy', 'data-ax-install-status',
    'data-ax-guide',
  ].map(name => [name, el(`[${name}]`)]));
  const products = ['ax', 'slim', 'admin'].map(p => el('[data-ax-product]', { axProduct: p }));
  const platforms = ['macos', 'linux', 'windows', 'wsl2'].map(os => el('[data-ax-os]', { axOs: os }));
  root.kids.push(...Object.values(parts), ...products, ...platforms);
  const click = (e: El) => (e.on.get('click') ?? []).forEach(fn => fn());
  const previous = { document: globalThis.document, window: globalThis.window, navigator: globalThis.navigator, Node: globalThis.Node };
  const set = (name: string, value: unknown) => Object.defineProperty(globalThis, name, { value, configurable: true, writable: true });
  set('Node', class {});
  set('navigator', { clipboard: { writeText: async (t: string) => { copied.push(t); } }, platform: 'MacIntel', userAgent: 'Mozilla/5.0 (Macintosh)', maxTouchPoints: 0 });
  set('document', { documentElement: { dataset: {} }, addEventListener() {}, querySelector: (s: string) => (s === '[data-ax-downloader]' ? root : null) });
  set('window', { setTimeout: () => 0, clearTimeout() {}, matchMedia: () => ({ matches: false }) });
  try {
    mountMinimalDownload();
    const [ax, slim, admin] = products;
    const [macos, linux, windows, wsl2] = platforms;
    assert.equal(ax.attrs['aria-pressed'], 'true');
    assert.equal(parts['data-ax-install-code'].textContent, 'curl -fsSL https://getax.wenneker.io/install.sh | sh');
    assert.equal(parts['data-ax-guide'].href, '/getting-started/installation/');

    click(parts['data-ax-platform-toggle']);
    click(slim);
    assert.equal(parts['data-ax-platform-panel'].hidden, false, 'choosing a product keeps the menu open');
    assert.equal(slim.attrs['aria-pressed'], 'true');
    assert.equal(ax.attrs['aria-pressed'], 'false');
    assert.deepEqual(platforms.map(b => b.hidden), [true, false, true, true]);
    assert.equal(linux.attrs['aria-pressed'], 'true');
    assert.equal(parts['data-ax-install-code'].textContent, 'curl -fsSL https://getax.wenneker.io/install-slim.sh | sh');
    assert.match(parts['data-ax-download-label'].textContent, /headless Linux/i);
    assert.equal(parts['data-ax-guide'].href, '/guides/headless-linux/');
    assert.equal(copied.length, 0, 'choosing a product does not copy yet');

    click(admin);
    assert.deepEqual(platforms.map(b => b.hidden), [false, false, false, true]);
    assert.equal(linux.attrs['aria-pressed'], 'true', 'Linux stays selected because ax-admin supports it');
    click(windows);
    await Promise.resolve();
    assert.equal(parts['data-ax-platform-panel'].hidden, true);
    assert.deepEqual(copied, ['irm https://getax.wenneker.io/install-admin.ps1 | iex']);
    assert.equal(parts['data-ax-install-code'].textContent, 'irm https://getax.wenneker.io/install-admin.ps1 | iex');
    assert.equal(parts['data-ax-download-label'].textContent, 'Copied');
    click(admin);
    assert.equal(parts['data-ax-download-label'].textContent, 'Copy ax-admin Windows install command');
    assert.equal(parts['data-ax-guide'].href, '/guides/remote-admin/');

    click(ax);
    assert.deepEqual(platforms.map(b => b.hidden), [false, false, false, false]);
    click(macos);
    await Promise.resolve();
    assert.equal(copied.at(-1), 'curl -fsSL https://getax.wenneker.io/install.sh | sh');
    click(ax);
    assert.equal(parts['data-ax-download-label'].textContent, 'Copy macOS install command');
    void wsl2;
  } finally {
    for (const [k, v] of Object.entries(previous)) set(k, v);
  }
});

test('copied install popup closes for another choice and fades out after 1.5 seconds', async () => {
  const timers: { fn: () => void; ms: number; dead: boolean }[] = [];
  const listeners = new Map<string, ((event: { target?: unknown; key?: string }) => void)[]>();
  function make(selectors: string[], dataset: Record<string, string> = {}) {
    const el: {
      selectors: string[];
      dataset: Record<string, string>;
      hidden: boolean;
      textContent: string;
      innerHTML: string;
      _kids: ReturnType<typeof make>[];
      _classes: Set<string>;
      classList: { add: (c: string) => void; remove: (c: string) => void; contains: (c: string) => boolean };
      _listeners: Map<string, (() => void)[]>;
      setAttribute: (name: string, value: string) => void;
      addEventListener: (type: string, fn: () => void) => void;
      contains: (node: unknown) => boolean;
      querySelector: (sel: string) => ReturnType<typeof make> | null;
      querySelectorAll: (sel: string) => ReturnType<typeof make>[];
    } = {
      selectors,
      dataset: { ...dataset },
      hidden: selectors.includes('[data-ax-install-panel]') || selectors.includes('[data-ax-platform-panel]'),
      textContent: '',
      innerHTML: '',
      _kids: [],
      _classes: new Set<string>(),
      classList: {
        add: (c: string) => el._classes.add(c),
        remove: (c: string) => el._classes.delete(c),
        contains: (c: string) => el._classes.has(c),
      },
      _listeners: new Map(),
      setAttribute() {},
      addEventListener(type: string, fn: () => void) {
        const list = el._listeners.get(type) ?? [];
        list.push(fn);
        el._listeners.set(type, list);
      },
      contains(node: unknown) {
        return node === el || el._kids.some(kid => kid.contains(node));
      },
      querySelector(sel: string) {
        return el.querySelectorAll(sel)[0] ?? null;
      },
      querySelectorAll(sel: string) {
        const found: ReturnType<typeof make>[] = [];
        const visit = (node: ReturnType<typeof make>) => {
          if (node.selectors.includes(sel)) found.push(node);
          node._kids.forEach(visit);
        };
        el._kids.forEach(visit);
        return found;
      },
    };
    return el;
  }
  const root = make(['[data-ax-downloader]']);
  const main = make(['[data-ax-download]']);
  const title = make(['[data-ax-download-label]']);
  const icon = make(['[data-ax-main-icon]']);
  const summary = make(['[data-ax-platform-label]']);
  const toggle = make(['[data-ax-platform-toggle]']);
  const panel = make(['[data-ax-platform-panel]']);
  const help = make(['[data-ax-platform-help]']);
  const install = make(['[data-ax-install-panel]']);
  const code = make(['[data-ax-install-code]']);
  const instruction = make(['[data-ax-install-instruction]']);
  const copy = make(['[data-ax-install-copy]']);
  const status = make(['[data-ax-install-status]']);
  root._kids.push(main, title, icon, summary, toggle, panel, help, install, code, instruction, copy, status);
  const previous = {
    document: globalThis.document,
    window: globalThis.window,
    navigator: globalThis.navigator,
    Node: globalThis.Node,
  };
  class DomNode {}
  function setGlobal(name: 'document' | 'window' | 'navigator' | 'Node', value: unknown) {
    Object.defineProperty(globalThis, name, { value, configurable: true, writable: true });
  }
  setGlobal('Node', DomNode);
  setGlobal('navigator', {
    clipboard: { writeText: async () => {} },
    platform: 'MacIntel',
    userAgent: 'Mozilla/5.0 (Macintosh)',
    maxTouchPoints: 0,
  });
  setGlobal('document', {
    documentElement: { dataset: {} },
    addEventListener(type: string, fn: (event: { target?: unknown; key?: string }) => void) {
      const list = listeners.get(type) ?? [];
      list.push(fn);
      listeners.set(type, list);
    },
    querySelector(sel: string) {
      return sel === '[data-ax-downloader]' ? root : null;
    },
  });
  setGlobal('window', {
    setTimeout(fn: () => void, ms: number) {
      timers.push({ fn, ms, dead: false });
      return timers.length;
    },
    clearTimeout(id: number) {
      const timer = timers[id - 1];
      if (timer) timer.dead = true;
    },
    matchMedia: () => ({ matches: false }),
  });
  function flush(ms: number) {
    const due = timers.filter(timer => !timer.dead && timer.ms === ms);
    for (const timer of due) {
      timer.dead = true;
      timer.fn();
    }
  }
  function click(el: { _listeners: Map<string, (() => void)[]> }) {
    for (const fn of el._listeners.get('click') ?? []) fn();
  }
  try {
    mountMinimalDownload();
    click(main);
    await Promise.resolve();
    assert.equal(install.hidden, false);
    click(toggle);
    assert.equal(install.hidden, true, 'other platforms closes the copied popup');
    click(main);
    await Promise.resolve();
    assert.equal(install.hidden, false);
    flush(1500);
    assert.equal(install.classList.contains('is-fading'), true);
    flush(900);
    assert.equal(install.hidden, true);
    click(main);
    await Promise.resolve();
    const outside = new DomNode();
    for (const fn of listeners.get('pointerdown') ?? []) fn({ target: outside });
    assert.equal(install.hidden, true, 'a click outside closes the copied popup');
  } finally {
    setGlobal('document', previous.document);
    setGlobal('window', previous.window);
    setGlobal('navigator', previous.navigator);
    setGlobal('Node', previous.Node);
  }
});

test('phone landing fits the viewport and the platform menu overlays with a blur', () => {
  const phone = '(max-width:600px)';
  for (const [label, sheet] of sheets) {
    same(decl(sheet, `${L}.ax-main`, 'padding', phone), '8px 20px 4px', label);
    same(decl(sheet, `${L}.ax-display-word`, 'font-size', phone), '48px', label);
    same(decl(sheet, `${L}.ax-platform-panel`, 'position', phone), 'absolute', label);
    same(decl(sheet, `${L}.ax-platform-panel`, 'bottom', phone), 'calc(100% + 8px)', label);
    same(decl(sheet, `${L}.ax-platform-panel`, 'backdrop-filter'), 'blur(18px)', label);
    same(decl(sheet, `${L}.ax-platform-panel`, 'background'), 'rgba(18,23,19,.62)', label);
  }
});

test('copied install button throws sparks so the copy stays visible', () => {
  for (const [label, sheet] of sheets) {
    const spark = sheet.keyframes['ax-copied-spark'];
    assert.equal(spark?.['0%']?.opacity, '1', label);
    assert.equal(spark?.['100%']?.opacity, '0', label);
    for (const button of [`${L}.ax-download-primary[data-copied="true"]::after`, `${L}.ax-install-command button[data-copied="true"]::after`]) {
      assert.match(decl(sheet, button, 'animation') ?? '', /^ax-copied-spark /, `${label}: ${button}`);
    }
    same(decl(sheet, `html[data-ax-motion='paused'] ${L}.ax-download-primary[data-copied="true"]::after`, 'animation'), 'none', label);
    assert.ok(!allDecls(sheet).some((d) => d.prop === 'border-radius' && d.value === '50%'), label);
  }
});

test('the CSS model reads a reformatted stylesheet the same way', () => {
  assert.deepEqual(parseCss(reformat(cssSource)), css);
  assert.ok(rulesFor(css, `${L}.ax-install-command`).length > 0);
});

test('install links keep the site base path (GitHub Pages mirror at /ax)', () => {
  // Static links in the component are prefixed with the Astro base.
  assert.doesNotMatch(download, /href="\/(?!\/)/, 'MinimalDownload must not hard-code root-relative links');
  assert.match(download, /data-ax-base=\{base\}/);
  // The guide link the script rewrites keeps the base too.
  type Node = { sel: string; dataset: Record<string, string>; hidden: boolean; textContent: string; innerHTML: string; href: string;
    classList: { add(): void; remove(): void }; setAttribute(): void; addEventListener(t: string, fn: () => void): void;
    on: Map<string, (() => void)[]>; contains(): boolean; querySelector(s: string): Node | null; querySelectorAll(s: string): Node[] };
  const kids: Node[] = [];
  const el = (sel: string, dataset: Record<string, string> = {}): Node => {
    const n: Node = { sel, dataset, hidden: false, textContent: '', innerHTML: '', href: '', on: new Map(),
      classList: { add() {}, remove() {} }, setAttribute() {},
      addEventListener(t, fn) { n.on.set(t, [...(n.on.get(t) ?? []), fn]); },
      contains: () => false,
      querySelector: s => kids.find(k => k.sel === s) ?? null,
      querySelectorAll: s => kids.filter(k => k.sel === s) };
    return n;
  };
  const root = el('[data-ax-downloader]', { axBase: '/ax' });
  for (const name of ['data-ax-download', 'data-ax-download-label', 'data-ax-main-icon', 'data-ax-platform-label',
    'data-ax-platform-toggle', 'data-ax-platform-panel', 'data-ax-platform-help', 'data-ax-install-panel',
    'data-ax-install-code', 'data-ax-install-instruction', 'data-ax-install-copy', 'data-ax-install-status',
    'data-ax-guide']) kids.push(el(`[${name}]`));
  const slim = el('[data-ax-product]', { axProduct: 'slim' });
  kids.push(slim);
  const previous = { document: globalThis.document, window: globalThis.window, navigator: globalThis.navigator };
  const set = (name: string, value: unknown) => Object.defineProperty(globalThis, name, { value, configurable: true, writable: true });
  set('navigator', { platform: 'MacIntel', userAgent: 'Mozilla/5.0 (Macintosh)', maxTouchPoints: 0 });
  set('document', { documentElement: { dataset: {} }, addEventListener() {}, querySelector: (s: string) => (s === '[data-ax-downloader]' ? root : null) });
  set('window', { setTimeout: () => 0, clearTimeout() {}, matchMedia: () => ({ matches: false }) });
  try {
    mountMinimalDownload();
    const guide = kids.find(k => k.sel === '[data-ax-guide]')!;
    assert.equal(guide.href, '/ax/getting-started/installation/');
    (slim.on.get('click') ?? []).forEach(fn => fn());
    assert.equal(guide.href, '/ax/guides/headless-linux/');
  } finally {
    for (const [k, v] of Object.entries(previous)) set(k, v);
  }
});
