import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { describe, it } from 'node:test';
import { readGlobalStyles } from '../test-support/globalStyles.ts';

// Stylesheet lint: record rows do not trap the list wheel; the memory vault list still scrolls.
describe('list scrolling (stylesheet lint)', () => {
  it('portal.css lets grouped lists in the page scroller pass the wheel on', () => {
    const portal = readFileSync(new URL('./portal.css', import.meta.url), 'utf8');
    const rule = portal.match(
      /html\[data-ax-layout="portal"\] \.page-split-main :is\(\.page-item-list, \.portal-data-list\)\s*\{[^}]*\}/,
    )?.[0] ?? '';
    assert.match(rule, /overflow:\s*visible/);
    assert.match(rule, /overscroll-behavior:\s*auto/);
  });

  it('the global styles keep the shared list open and the memory vault list scrollable', () => {
    const index = readGlobalStyles();
    const list = index.match(/\.page-item-list\s*\{[^}]+\}/)?.[0] ?? '';
    assert.equal(list.includes('overscroll-behavior: contain'), false);
    const vault = index.match(/\.memory-vault-list\s*\{[^}]+\}/)?.[0] ?? '';
    assert.match(vault, /overflow-y:\s*auto/);
  });
});
