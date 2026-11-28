import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { neighborId, neighborNav } from './rowNeighbors.ts';

// Rendered rows are newest first: c is the top row.
const rows = ['c', 'b', 'a'];

test('the top row can move to the next row but not to a previous one', () => {
  assert.deepEqual(neighborNav(rows, 'c'), { prev: false, next: true });
  assert.equal(neighborId(rows, 'c', 1), 'b');
  assert.equal(neighborId(rows, 'c', -1), null);
});

test('the bottom row can only move back', () => {
  assert.deepEqual(neighborNav(rows, 'a'), { prev: true, next: false });
  assert.equal(neighborId(rows, 'a', -1), 'b');
});

test('a selection hidden by a filter has no neighbors', () => {
  assert.deepEqual(neighborNav(rows, 'zz'), { prev: false, next: false });
  assert.deepEqual(neighborNav(rows, null), { prev: false, next: false });
  assert.equal(neighborId(rows, 'zz', 1), null);
});

test('the inspector buttons use the rendered rows, not the chronological index', () => {
  const source = readFileSync(new URL('../components/McpTraceLive.tsx', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /disabled=\{selectedIndex <= 0\}/);
  assert.doesNotMatch(source, /disabled=\{selectedIndex >= entries\.length - 1\}/);
  assert.match(source, /disabled=\{!nav\.prev\}/);
  assert.match(source, /disabled=\{!nav\.next\}/);
});
