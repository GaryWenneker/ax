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

test('CMY interference fades horizontally from the edge and uses continuous tile loops', () => {
  assert.match(home, /ax-halftone-screen-three/);
  assert.match(home, /<MotionControl\s*\/>/);
  for (const [label, sheet] of sheets) {
    const at = (selector: string, prop: string, media = '') => decl(sheet, `${L}${selector}`, prop, media);
    const screens = [
      ['one', '#00ffff', '26px 26px', 'ax-moire-spin 180s linear infinite', undefined],
      ['two', '#ff00ff', '26.8px 26.8px', 'ax-moire-spin 240s linear infinite reverse', '-11s'],
      ['three', '#ffff00', '25.4px 25.4px', 'ax-moire-spin 320s linear infinite', '-15.5s'],
    ] as const;
    for (const [screen, ink, pitch, animation, delay] of screens) {
      const sel = `.ax-halftone-screen-${screen}`;
      assert.ok(at(sel, 'background-image')?.includes(normalizeValue(ink)), `${label}: ${screen} ink`);
      same(at(sel, 'background-size'), pitch, `${label}: ${screen} pitch`);
      same(at(sel, 'animation'), animation, `${label}: ${screen} animation`);
      assert.equal(at(sel, 'animation-delay'), delay && normalizeValue(delay), `${label}: ${screen} delay`);
    }
    same(at('.ax-halftone-dots', 'mix-blend-mode'), 'multiply', label);
    const tint = at('.ax-halftone-grid::after', 'background') ?? '';
    assert.ok(tint.includes(normalizeValue('color-mix(in srgb,var(--ax-lime) 82%,transparent)')), `${label}: lime tint`);
    same(at('.ax-halftone-grid::after', 'mix-blend-mode'), 'color', label);
    assert.ok((at('.ax-halftone', 'mask-image') ?? '').startsWith(
      normalizeValue('linear-gradient(90deg,transparent 0%,transparent 18%,rgba(0,0,0,.22) 36%,#000 58%,#000 100%)')), `${label}: edge fade`);
    same(at('.ax-halftone-sharp', 'filter'), 'blur(.4px)', label);
    same(at('.ax-halftone-soft', 'animation'), 'ax-moire-breath 24s ease-in-out infinite', label);
    assert.deepEqual(sheet.keyframes['ax-moire-breath'], {
      '0%': { filter: 'blur(4px)' }, '100%': { filter: 'blur(4px)' }, '50%': { filter: normalizeValue('blur(1.2px)') },
    }, label);
    assert.deepEqual(sheet.keyframes['ax-moire-spin'], { '100%': { transform: 'rotate(1turn)' } }, label);
    assert.equal(sheet.keyframes['ax-moire-magenta'], undefined, label);
    assert.equal(sheet.keyframes['ax-moire-yellow'], undefined, label);
    // Reduced motion and the pause control stop the backdrop.
    for (const part of ['.ax-halftone-dots', '.ax-halftone-grid']) {
      same(decl(sheet, `html:not([data-ax-motion='running']) ${L}${part}`, 'animation-play-state', '(prefers-reduced-motion:reduce)'), 'paused', `${label}: reduced motion ${part}`);
      same(decl(sheet, `html[data-ax-motion='paused'] ${L}${part}`, 'animation-play-state'), 'paused', `${label}: paused ${part}`);
    }
    assert.match(at('.ax-install-command', 'position') ?? '', /^absolute$/, label);
    same(at('.ax-install-command', 'backdrop-filter'), 'blur(18px)', label);
    same(at('.ax-halftone', 'inset', '(max-width:600px)'), '-18%', label);
    same(at('.ax-halftone', 'opacity', '(max-width:600px)'), '.32', label);
    for (const d of allDecls(sheet)) {
      assert.doesNotMatch(d.value, /infinite alternate|translate3d/, `${label}: ${d.selector}`);
      assert.ok(!(d.prop === 'border-radius' && d.value === '50%'), `${label}: no round dots in ${d.selector}`);
      assert.ok(!(d.prop.startsWith('background') && /url\(/.test(d.value)), `${label}: no images in ${d.selector}`);
      assert.doesNotMatch(d.selector, /ax-halftone-rosette/, label);
      if (/data-ax-install-panel\]:not\(\[hidden\]\)/.test(d.selector)) assert.notEqual(d.prop, 'overflow-y', label);
    }
  }
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
  assert.ok(rulesFor(css, `${L}.ax-halftone`).length > 0);
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
