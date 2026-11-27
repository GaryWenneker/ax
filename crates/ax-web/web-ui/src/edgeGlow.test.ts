import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
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


describe('B5 selected-node orbit glow is off in portal', () => {
  it('disables orbit and breathe animations', () => {
    assert.match(
      portalCss,
      /html\[data-ax-layout="portal"\][\s\S]*ax-node-orbit|page-item-node::before[\s\S]*animation:\s*none/,
    );
    assert.match(portalCss, /page-item-node::(before|after)[\s\S]*animation:\s*none/);
  });
});

describe('B4 no dead wave component or spec remains', () => {
  it('has no HeaderWaves component', () => {
    assert.equal(existsSync(new URL('./components/HeaderWaves.tsx', import.meta.url)), false);
  });
  it('has no e2e spec that expects titlebar waves', () => {
    const e2eDir = new URL('../e2e/', import.meta.url);
    for (const name of readdirSync(e2eDir).filter((n) => n.endsWith('.spec.ts'))) {
      const text = readFileSync(new URL(name, e2eDir), 'utf8');
      assert.equal(text.includes('cc-waves'), false, name);
    }
  });
  it('has no wave styles left in the stylesheets', () => {
    assert.equal(indexCss.includes('.cc-waves'), false);
    assert.equal(portalCss.includes('.cc-waves'), false);
  });
});
