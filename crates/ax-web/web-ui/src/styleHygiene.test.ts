import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { appStylesheets, cssRules } from '../test-support/cssRules.ts';
import { contrastRatioHex, THEMES } from './lib/themes.ts';

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

describe('Logging direction marks', () => {
  const rules = [...sheets.values()].flatMap((css) => cssRules(css)).filter((rule) => rule.conditions.length === 0);
  const tokenIn = (selector: string, name: string) => {
    let value: string | undefined;
    for (const rule of rules) if (rule.selectors.includes(selector) && rule.declarations.has(name)) value = rule.declarations.get(name);
    return value;
  };
  /** The colour a direction mark gets in a theme, following one level of var(). */
  const ink = (modifier: string, themeId: string) => {
    const raw = tokenIn(modifier, '--dir-ink') ?? '';
    const ref = /^var\((--[\w-]+)\)$/.exec(raw);
    if (!ref) return raw;
    return tokenIn(`html[data-ax-theme="${themeId}"]`, ref[1]) ?? tokenIn(':root', ref[1]) ?? '';
  };

  for (const theme of THEMES) {
    it(`reach 3:1 against the page and panel in the ${theme.label} theme`, () => {
      const low: string[] = [];
      for (const dir of ['in', 'out', 'internal']) {
        const color = ink(`.mcp-dir-legend-item--${dir}`, theme.id);
        assert.match(color, /^#[0-9a-f]{6}$/i, `${dir} resolves to a hex colour`);
        for (const bg of [theme.bg, theme.bgPanel]) {
          const ratio = contrastRatioHex(color, bg);
          if (ratio < 3) low.push(`${dir} ${color} on ${bg}: ${ratio.toFixed(2)}`);
        }
      }
      assert.deepEqual(low, []);
    });
  }
});

describe('keyframes', () => {
  it('declares only keyframes that some animation runs', () => {
    const named = [...allCss.matchAll(/@keyframes\s+([\w-]+)/g)].map((m) => m[1]);
    const animations = [...sheets.values()].flatMap((css) => cssRules(css))
      .flatMap((rule) => [rule.declarations.get('animation') ?? '', rule.declarations.get('animation-name') ?? ''])
      .join(' ');
    const unused = named.filter((name) => !new RegExp(`(^|[\\s,])${name}($|[\\s,;!])`).test(animations));
    assert.deepEqual(unused, []);
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
