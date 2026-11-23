import assert from 'node:assert/strict';
import test from 'node:test';
import { formatSpecDate, sortSpecs, specVersionLabel } from './specSort.ts';

const rows = [
  { id: 'b', title: 'Beta', updatedAt: 1_700_000_000_000 },
  { id: 'a', title: 'Alpha', updatedAt: 1_800_000_000_000 },
  { id: 'c', title: 'Alpha', updatedAt: 1_600_000_000_000 },
];

test('newest update is first', () => {
  const sorted = sortSpecs(rows, 'newest');
  assert.deepEqual(sorted.map((row) => row.id), ['a', 'b', 'c']);
  assert.equal(rows[0].id, 'b');
});

test('oldest update is first', () => {
  assert.deepEqual(sortSpecs(rows, 'oldest').map((row) => row.id), ['c', 'b', 'a']);
});

test('title order puts the newer copy first when titles match', () => {
  assert.deepEqual(sortSpecs(rows, 'title').map((row) => row.id), ['a', 'c', 'b']);
});

test('a missing date sorts as the oldest', () => {
  const mixed = [...rows, { id: 'z', title: 'Zed' }];
  assert.equal(sortSpecs(mixed, 'newest').at(-1)?.id, 'z');
  assert.equal(sortSpecs(mixed, 'oldest')[0].id, 'z');
});

test('formats a millisecond timestamp in UTC', () => {
  assert.equal(formatSpecDate(Date.UTC(2026, 10, 21, 10, 41), 'UTC'), '21 Nov 2026, 10:41');
});

test('a blank timestamp has no label', () => {
  assert.equal(formatSpecDate(0), '');
  assert.equal(formatSpecDate(Number.NaN), '');
});

test('the version label names the draft, the source, and the base', () => {
  assert.equal(
    specVersionLabel({ latestVersion: 3, latestStatus: 'draft', latestSource: 'edit', latestBasedOn: 2 }),
    'v3 · draft · edit · based on v2',
  );
  assert.equal(specVersionLabel({ latestVersion: 1, latestStatus: 'approved', latestSource: 'plan_mode', latestBasedOn: null }), 'v1 · approved · plan_mode');
  assert.equal(specVersionLabel({ latestVersion: 0 }), '');
});
