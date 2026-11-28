import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { LruCache } from './lruCache.ts';

test('the cache keeps at most max entries and drops the least recently used', () => {
  const c = new LruCache<string, number>(2);
  c.set('a', 1);
  c.set('b', 2);
  assert.equal(c.get('a'), 1); // a is now most recent
  c.set('c', 3);
  assert.equal(c.size, 2);
  assert.equal(c.get('b'), undefined);
  assert.equal(c.get('a'), 1);
  assert.equal(c.get('c'), 3);
});

test('setting an existing key replaces it without growing the cache', () => {
  const c = new LruCache<string, number>(2);
  c.set('a', 1);
  c.set('a', 2);
  assert.equal(c.size, 1);
  assert.equal(c.get('a'), 2);
});

test('memory diffs are cached in a bounded LRU and only when pinned to a commit', () => {
  const source = readFileSync(new URL('../components/MemoryDiffHover.tsx', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /new Map<string, MemoryDiff>\(\)/);
  assert.match(source, /new LruCache<string, MemoryDiff>\(DIFF_CACHE_MAX\)/);
  assert.match(source, /if \(commit\) cache\.set\(key, result\)/);
});
