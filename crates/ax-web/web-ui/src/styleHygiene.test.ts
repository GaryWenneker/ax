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

describe('viewport breakpoints', () => {
  it('never uses one width as both a max-width and a min-width bound', () => {
    const bounds = (kind: 'max' | 'min') => {
      const found = new Map<number, string[]>();
      for (const [file, css] of sheets) {
        for (const query of css.matchAll(/@media\b[^{]*/g)) {
          for (const m of query[0].matchAll(new RegExp(`\\(${kind}-width:\\s*(\\d+)px\\)`, 'g'))) {
            const width = Number(m[1]);
            found.set(width, [...(found.get(width) ?? []), file]);
          }
        }
      }
      return found;
    };
    const max = bounds('max');
    const overlaps = [...bounds('min').keys()].filter((width) => max.has(width))
      .map((width) => `${width}px in ${max.get(width)?.join(', ')}`);
    assert.deepEqual(overlaps, [], 'a viewport exactly this wide gets both sets of rules; use max-width: N-1px');
  });
});
