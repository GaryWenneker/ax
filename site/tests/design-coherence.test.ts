import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const file = (relative: string) => readFileSync(path.join(root, relative), 'utf8');

test('docs use the landing graphite/lime theme and responsive moving overprint', () => {
  const theme = file('site/src/styles/theme.css');
  const header = file('site/src/components/Header.astro');
  const title = file('site/src/components/SiteTitle.astro');
  const astro = file('site/astro.config.mjs');
  assert.match(theme, /--ax-lime:#b9ff6a/);
  assert.match(theme, /--sl-color-accent: #b9ff6a/);
  assert.match(header, /import '..\/styles\/minimal-home.css'/);
  assert.match(header, /class="ax-doc-moire ax-landing"/);
  for (const screen of ['one', 'two', 'three']) assert.ok(header.includes(`ax-halftone-screen-${screen}`));
  assert.match(theme, /content-panel\{background:rgba\(8,10,9,\.88\)/);
  assert.match(theme, /data-theme='light'.*content-panel\{background:rgba\(255,255,255,\.94\)/);
  assert.match(title, /cg-site-title__wordmark">Ax<span>_<\/span>/);
  assert.match(astro, /@fontsource-variable\/oxanium\/wght\.css/);
});

test('Command Center uses the same Ax_ wordmark instead of old image assets', () => {
  const app = file('crates/ax-web/web-ui/src/App.tsx');
  const css = file('crates/ax-web/web-ui/src/index.css');
  assert.match(app, /className="ax-app-wordmark" aria-label="Ax home">Ax<span aria-hidden="true">_<\/span>/);
  assert.match(app, /className="ax-app-wordmark ax-app-wordmark--sidebar"/);
  assert.doesNotMatch(app, /src="\/ax-mark\.png"|src="\/ax-logo\.png"/);
  assert.match(app, /<a href="\/" className="ax-app-wordmark"/);
  assert.match(css, /font-family:"Archivo Variable"/);
  assert.match(css, /letter-spacing:-\.085em/);
  assert.match(css, /\.ax-app-wordmark > span\{\s*color:#b9ff6a/);
});
