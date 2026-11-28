import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const file = (relative: string) => readFileSync(path.join(root, relative), 'utf8');

test('docs use the calm theme: system type, thin lines, one blue accent, no backdrop', () => {
  const theme = file('site/src/styles/theme.css');
  const header = file('site/src/components/Header.astro');
  const head = file('site/src/components/Head.astro');
  const social = file('site/src/components/SocialIcons.astro');
  const title = file('site/src/components/SiteTitle.astro');
  const astro = file('site/astro.config.mjs');
  assert.match(theme, /--ax-accent: #8fbfff;/);
  assert.match(theme, /--ax-accent-fill: #1372da;/);
  assert.match(theme, /--sl-color-accent: #1372da;/);
  assert.match(theme, /--sl-font:\s*-apple-system, BlinkMacSystemFont, "Segoe UI"/);
  assert.match(theme, /--cg-paper: #0a0a0a;/);
  assert.doesNotMatch(header + head + social, /DeckBackdrop|ScrollReveal|MotionControl/);
  assert.doesNotMatch(theme, /animation|@keyframes|transition|backdrop-filter|box-shadow:(?! none)|gradient|ax-reveal/);
  assert.doesNotMatch(theme + astro + title, /fontsource|Oxanium|Archivo|IBM Plex/);
  // White text on the accent fill keeps 4.5:1.
  assert.match(theme, /\.sl-banner,\s*\.sl-banner a \{\s*color: #fff;/);
  assert.match(title, /cg-site-title__wordmark"><AxMark \/>/);
});

test('Command Center uses the Ax logo mark instead of old image assets', () => {
  const app = file('crates/ax-web/web-ui/src/App.tsx');
  const css = file('crates/ax-web/web-ui/src/styles/23-brand.css');
  assert.match(app, /className="ax-app-wordmark" aria-label="Ax home"><AxMark \/>/);
  assert.match(app, /className="ax-app-wordmark ax-app-wordmark--sidebar"/);
  assert.doesNotMatch(app, /src="\/ax-mark\.png"|src="\/ax-logo\.png"/);
  assert.match(app, /<a href="\/" className="ax-app-wordmark"/);
  assert.match(css, /font-family:"Archivo Variable"/);
  assert.match(css, /letter-spacing:-\.085em/);
  assert.match(css, /\.ax-app-wordmark > span\{\s*color:#b9ff6a/);
  assert.match(css, /\.ax-app-wordmark \.ax-mark\{display:block;height:\.76em;width:auto\}/);
});
