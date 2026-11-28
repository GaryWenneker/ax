import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { createRefreshingCache } from './refreshingCache.ts';

function counter() {
  let calls = 0;
  return { load: () => Promise.resolve(++calls), calls: () => calls };
}

test('a fresh entry is reused and a stale one is reloaded', async () => {
  let now = 0;
  const c = counter();
  const cache = createRefreshingCache(c.load, 5000, () => now);
  assert.equal(await cache.get(), 1);
  now = 4999;
  assert.equal(await cache.get(), 1);
  now = 5000;
  assert.equal(await cache.get(), 2);
  assert.equal(c.calls(), 2);
});

test('clear drops the entry so the next get loads again', async () => {
  const c = counter();
  const cache = createRefreshingCache(c.load, 60_000, () => 0);
  await cache.get();
  cache.clear();
  assert.equal(await cache.get(), 2);
});

test('a failed load is not cached, so the next get retries', async () => {
  let fail = true;
  const cache = createRefreshingCache(() => (fail ? Promise.reject(new Error('down')) : Promise.resolve('ok')), 60_000, () => 0);
  await assert.rejects(cache.get());
  fail = false;
  assert.equal(await cache.get(), 'ok');
});

test('the link picker refreshes targets on open, on workspace switch and after an error', () => {
  const source = readFileSync(new URL('../components/LinkPicker.tsx', import.meta.url), 'utf8');
  assert.match(source, /createRefreshingCache/);
  assert.match(source, /WORKSPACE_SWITCHED/);
  assert.doesNotMatch(source, /if \(!active \|\| targets\) return;/);
  assert.match(source, /useLinkTargets\(true, active\)/);
});
