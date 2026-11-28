import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

// Deploy buttons are images inside links. The docs zoom every image on click, which used to
// swallow the click: tapping "Deploy to Azure" enlarged the button instead of opening Azure.
const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = (...parts: string[]) => readFileSync(path.join(site, ...parts), 'utf8');
const config = read('astro.config.mjs');
const css = read('src/styles/theme.css');
const guide = read('src/content/docs/guides/headless-linux.md');
const installation = read('src/content/docs/getting-started/installation.md');

/** Runs the head script that installs the image zoom and returns its document click handler. */
function zoomClickHandler(): (event: { target: unknown; preventDefault: () => void }) => void {
  const scripts = [...config.matchAll(/content: `(\(function\(\)\{[\s\S]*?\}\)\(\);?)`/g)].map(m => m[1]);
  const zoom = scripts.find(s => s.includes('ax-lightbox'));
  assert.ok(zoom, 'image zoom script in astro.config.mjs');
  let handler: ((event: { target: unknown; preventDefault: () => void }) => void) | undefined;
  const document = {
    addEventListener(type: string, fn: typeof handler) { if (type === 'click') handler = fn; },
    removeEventListener() {},
    createElement: () => ({ className: '', innerHTML: '', classList: { add() {}, remove() {} }, querySelector: () => ({ addEventListener() {} }), addEventListener() {} }),
    body: { appendChild() {} },
  };
  vm.runInNewContext(zoom, { document, requestAnimationFrame: (fn: () => void) => fn() });
  assert.ok(handler, 'zoom script registers a click handler');
  return handler;
}

function image(inLink: boolean) {
  return {
    tagName: 'IMG', src: 'https://example.com/button.svg', alt: 'Deploy',
    closest: (selector: string) => (selector === '.sl-markdown-content' || (selector === 'a' && inLink) ? {} : null),
  };
}

test('a click on a linked image follows the link instead of zooming', () => {
  const onClick = zoomClickHandler();
  let prevented = false;
  onClick({ target: image(true), preventDefault: () => { prevented = true; } });
  assert.equal(prevented, false, 'the deploy link must open');
  onClick({ target: image(false), preventDefault: () => { prevented = true; } });
  assert.equal(prevented, true, 'a plain screenshot still zooms');
});

test('linked images look like buttons, not zoomable screenshots', () => {
  const rule = css.match(/\.sl-markdown-content a img\s*\{([^}]*)\}/)?.[1] ?? '';
  assert.match(rule, /cursor:\s*pointer/);
  assert.match(rule, /border:\s*0/);
  assert.match(rule, /margin:\s*0/);
  assert.match(rule, /display:\s*inline-block/);
});

test('deploy buttons are one uniform button with the provider logo, outside table cells', () => {
  for (const [name, text] of [['headless-linux', guide], ['installation', installation]] as const) {
    const buttons = text.split('\n').filter(line => line.includes('class="ax-deploy"'));
    assert.ok(buttons.length > 0, `${name} has deploy buttons`);
    for (const line of buttons) {
      assert.match(line, /^<a class="ax-deploy" data-provider="(azure|render|koyeb)" href="https:\/\/[^"]+" target="_blank" rel="noopener noreferrer">Deploy to (Azure|Render|Koyeb)<\/a>$/, `${name}: ${line.slice(0, 60)}`);
    }
    // No third-party badge images: they came in three different sizes.
    assert.doesNotMatch(text, /deploytoazurebutton|deploy-to-render-button|koyeb\.com\/static\/images/);
  }
  for (const platform of ['Azure Container Apps', 'Render', 'Koyeb']) {
    assert.match(guide, new RegExp(`^### ${platform}$`, 'm'), `guide heading for ${platform}`);
  }
  // Same size everywhere, logo from a local file.
  assert.match(css, /\.sl-markdown-content \.ax-deploy \{[^}]*width: 13rem;[^}]*height: 44px;/);
  for (const provider of ['azure', 'render', 'koyeb']) {
    assert.match(css, new RegExp(`\\.ax-deploy\\[data-provider="${provider}"\\] \\{ --ax-deploy-logo: url\\("/logos/${provider}\\.svg"\\); \\}`));
    assert.ok(read('public', 'logos', `${provider}.svg`).startsWith('<svg'), `${provider} logo`);
  }
  const home = read('src/components/CloudDeploy.astro');
  assert.match(home, /class="ax-deploy"[^>]*data-provider=\{d\.provider\}/);
});
