import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { describe, it } from 'node:test';

const app = readFileSync(new URL('./App.tsx', import.meta.url), 'utf8');
const indexCss = readFileSync(new URL('./index.css', import.meta.url), 'utf8');
const portalCss = readFileSync(new URL('./portal.css', import.meta.url), 'utf8');

describe('B1 App shell never mounts titlebar waves', () => {
  it('does not import HeaderWaves', () => {
    assert.equal(app.includes('HeaderWaves'), false);
  });
});

describe('B2 wave ribbon height is zero', () => {
  it('assigns --titlebar-wave-h only as 0px', () => {
    const values = [...indexCss.matchAll(/--titlebar-wave-h:\s*([^;]+);/g)].map((m) => m[1].trim());
    assert.ok(values.length >= 1, 'index.css must set --titlebar-wave-h');
    for (const value of values) {
      assert.equal(value, '0px');
    }
  });
});

describe('B3 leftover wave markup does not paint', () => {
  it('hides .cc-waves under portal layout', () => {
    assert.match(portalCss, /html\[data-ax-layout="portal"\]\s+\.cc-waves\s*\{[^}]*display:\s*none/s);
  });
});

describe('B5 selected-node orbit glow is off in portal', () => {
  it('disables orbit and breathe animations', () => {
    assert.match(
      portalCss,
      /html\[data-ax-layout="portal"\][\s\S]*ax-node-orbit|page-item-node::before[\s\S]*animation:\s*none/,
    );
    assert.match(portalCss, /page-item-node::(before|after)[\s\S]*animation:\s*none/);
  });
});
