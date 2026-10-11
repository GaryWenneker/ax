import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { describe, it } from 'node:test';

const portal = readFileSync(new URL('./portal.css', import.meta.url), 'utf8');
const index = readFileSync(new URL('./index.css', import.meta.url), 'utf8');

describe('record rows do not trap the list wheel', () => {
  it('L1 grouped lists inside the page scroller are not scrollports', () => {
    assert.match(
      portal,
      /html\[data-ax-layout="portal"\] \.page-split-main :is\(\.page-item-list, \.portal-data-list\)\s*\{[^}]*overflow:\s*visible/s,
    );
    assert.match(
      portal,
      /html\[data-ax-layout="portal"\] \.page-split-main :is\(\.page-item-list, \.portal-data-list\)\s*\{[^}]*overscroll-behavior:\s*auto/s,
    );
  });

  it('L1 the shared list does not contain the wheel', () => {
    const block = index.match(/\.page-item-list\s*\{[^}]+\}/)?.[0] ?? '';
    assert.equal(block.includes('overscroll-behavior: contain'), false);
  });

  it('L2 the memory vault list stays a scrollport', () => {
    const block = index.match(/\.memory-vault-list\s*\{[^}]+\}/)?.[0] ?? '';
    assert.match(block, /overflow-y:\s*auto/);
  });
});
