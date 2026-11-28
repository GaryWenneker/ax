import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildPathsFromTokens, fitAltsIntoBand, histBins, type TokenPath } from './tokenPath.ts';

test('fitting a very long alternate path into the band does not overflow the call stack', () => {
  const n = 300_000;
  const alt: TokenPath = {
    id: 'alt-0',
    label: 'Alt path 1',
    matched: false,
    score: 0.5,
    points: Array.from({ length: n }, (_, i) => ({ position: i, token: 'x', weight: 0.1 + (i % 7) / 10 })),
  };
  const [fitted] = fitAltsIntoBand([alt], 0.05, 1);
  assert.equal(fitted.points.length, n);
  for (const p of [fitted.points[0], fitted.points[n - 1]]) {
    assert.ok(p.weight >= 0.05 && p.weight <= 1, `weight ${p.weight} stays in the band`);
  }
  assert.equal(fitted.points[0].weight, 0.05, 'the lowest alt weight maps to the bottom of the band');
});

test('a long response builds paths without throwing', () => {
  const tokens = Array.from({ length: 150_000 }, (_, i) => `w${i % 500}`);
  const { matched, alts } = buildPathsFromTokens(tokens, [], 8);
  assert.equal(matched.points.length, tokens.length);
  const fitted = fitAltsIntoBand(alts, 0.05, 1);
  assert.equal(fitted.length, 8);
});

test('histogram counts every weight once', () => {
  const bins = histBins([0.05, 0.1, 0.5, 1, 1], 0.05, 1, 4);
  assert.equal(bins.reduce((a, b) => a + b, 0), 5);
});
