import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { allDecls, decl, normalizeValue, parseCss, reformat, type Sheet } from './css-model.ts';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const file = (relative: string) => readFileSync(path.join(root, relative), 'utf8');
const hasVar = (sheet: Sheet, name: string, value: string) =>
  allDecls(sheet).some((d) => d.prop === name && d.value === normalizeValue(value));

test('docs use the landing graphite/lime theme and responsive moving overprint', () => {
  const themeSource = file('site/src/styles/theme.css');
  const header = file('site/src/components/Header.astro');
  const title = file('site/src/components/SiteTitle.astro');
  const astro = file('site/astro.config.mjs');
  assert.match(header, /import '..\/styles\/minimal-home.css'/);
  assert.match(header, /class="ax-doc-moire ax-landing"/);
  for (const screen of ['one', 'two', 'three']) assert.ok(header.includes(`ax-halftone-screen-${screen}`));
  assert.match(header, /axParkDocMoire/);
  // Checked on the file as written and on a formatter's version of it.
  for (const [label, theme] of [['as written', parseCss(themeSource)], ['reformatted', parseCss(reformat(themeSource))]] as const) {
    assert.ok(hasVar(theme, '--ax-lime', '#b9ff6a'), `${label}: --ax-lime`);
    assert.ok(hasVar(theme, '--sl-color-accent', '#b9ff6a'), `${label}: --sl-color-accent`);
    const starts = (value: string | undefined, prefix: string, what: string) =>
      assert.ok((value ?? '').startsWith(normalizeValue(prefix)), `${label}: ${what} is ${value}`);
    starts(decl(theme, '.main-pane .content-panel::before', 'background'), 'linear-gradient(90deg,rgba(8,10,9,.94)', 'dark content panel');
    starts(decl(theme, ":root[data-theme='light'] .main-pane .content-panel::before", 'background'), 'linear-gradient(90deg,rgba(255,255,255,.96)', 'light content panel');
    assert.equal(decl(theme, '.right-sidebar-panel::before', 'background'), normalizeValue('rgba(18,22,20,.45)'), label);
    assert.equal(decl(theme, '.right-sidebar-panel::before', 'mask-composite'), 'intersect', label);
    assert.deepEqual(['z-index', 'background', 'pointer-events'].map((p) => decl(theme, '.right-sidebar', p)), ['4', 'transparent', 'none'], label);
  }
  assert.match(title, /cg-site-title__wordmark">Ax<span>_<\/span>/);
  assert.match(astro, /@fontsource-variable\/oxanium\/wght\.css/);
});

test('Command Center uses the same Ax_ wordmark instead of old image assets', () => {
  const app = file('crates/ax-web/web-ui/src/App.tsx');
  const css = parseCss(file('crates/ax-web/web-ui/src/index.css'));
  assert.match(app, /className="ax-app-wordmark" aria-label="Ax home">Ax<span aria-hidden="true">_<\/span>/);
  assert.match(app, /className="ax-app-wordmark ax-app-wordmark--sidebar"/);
  assert.doesNotMatch(app, /src="\/ax-mark\.png"|src="\/ax-logo\.png"/);
  assert.match(app, /<a href="\/" className="ax-app-wordmark"/);
  assert.match(decl(css, '.ax-app-wordmark', 'font-family') ?? '', /^'Archivo Variable'/);
  assert.equal(decl(css, '.ax-app-wordmark', 'letter-spacing'), normalizeValue('-.085em'));
  assert.equal(decl(css, '.ax-app-wordmark > span', 'color'), '#b9ff6a');
});
