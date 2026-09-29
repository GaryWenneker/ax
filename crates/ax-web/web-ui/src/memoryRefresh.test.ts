import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { memoryListChanged } from './memoryRefresh.ts';

const row = (id: string, updated_at = 1, extra: { enabled?: boolean; score?: number } = {}) => ({
  id,
  updated_at,
  ...extra,
});

describe('memory list refresh', () => {
  it('reports no change for the same rows', () => {
    assert.equal(memoryListChanged([row('a'), row('b')], [row('a'), row('b')]), false);
  });

  it('detects a new memory', () => {
    assert.equal(memoryListChanged([row('a')], [row('b'), row('a')]), true);
  });

  it('detects a removed memory', () => {
    assert.equal(memoryListChanged([row('a'), row('b')], [row('a')]), true);
  });

  it('detects an edited memory', () => {
    assert.equal(memoryListChanged([row('a', 1)], [row('a', 2)]), true);
  });

  it('detects a toggled memory', () => {
    assert.equal(
      memoryListChanged([row('a', 1, { enabled: true })], [row('a', 1, { enabled: false })]),
      true,
    );
  });

  it('detects a reordered list', () => {
    assert.equal(memoryListChanged([row('a'), row('b')], [row('b'), row('a')]), true);
  });

  it('detects a changed recall score', () => {
    assert.equal(
      memoryListChanged([row('a', 1, { score: 0.5 })], [row('a', 1, { score: 0.7 })]),
      true,
    );
  });
});
