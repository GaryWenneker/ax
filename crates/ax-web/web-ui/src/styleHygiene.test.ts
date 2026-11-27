import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { appStylesheets, cssRules } from '../test-support/cssRules.ts';
import { THEMES } from './lib/themes.ts';

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

describe('theme selectors', () => {
  it('only target theme ids that applyTheme can set', () => {
    const known = new Set(THEMES.map((theme) => theme.id));
    const unknown = new Set<string>();
    for (const m of allCss.matchAll(/data-ax-theme="([^"]+)"/g)) if (!known.has(m[1])) unknown.add(m[1]);
    assert.deepEqual([...unknown], []);
  });
});

describe('reduced motion', () => {
  const reduced = [...sheets.values()].flatMap((css) => cssRules(css))
    .filter((rule) => rule.conditions.some((c) => /prefers-reduced-motion:\s*reduce/.test(c)));

  it('stops every animation and transition when the user asks for reduced motion', () => {
    const reset = reduced.find((rule) => rule.selectors.includes('*'));
    assert.ok(reset, 'a global * rule under prefers-reduced-motion: reduce');
    assert.equal(reset.declarations.get('animation-iteration-count'), '1 !important');
    assert.match(reset.declarations.get('animation-duration') ?? '', /^0?\.01ms !important$/);
    assert.match(reset.declarations.get('transition-duration') ?? '', /^0?\.01ms !important$/);
    for (const pseudo of ['*::before', '*::after']) assert.ok(reset.selectors.includes(pseudo), pseudo);
  });

  it('keeps only progress spinners turning', () => {
    const kept = reduced.filter((rule) => rule.declarations.get('animation-iteration-count') === 'infinite !important');
    const selectors = kept.flatMap((rule) => rule.selectors);
    assert.ok(selectors.length > 0, 'spinners are re-enabled explicitly');
    for (const selector of selectors) assert.match(selector, /spin|ship-step-icon--live/, selector);
  });
});
