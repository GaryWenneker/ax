import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { appStylesheets } from '../test-support/cssRules.ts';

const sheets = appStylesheets();
const allCss = [...sheets.values()].join('\n');

describe('custom properties', () => {
  it('declares no breakpoint token, since media queries cannot read custom properties', () => {
    const declared = [...allCss.matchAll(/(--bp-[\w-]+)\s*:/g)].map((m) => m[1]);
    assert.deepEqual(declared, []);
  });
});
