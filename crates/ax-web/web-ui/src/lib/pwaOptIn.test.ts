import assert from 'node:assert/strict';
import { test } from 'node:test';
import { resolvePwaOptIn } from './pwaOptIn.ts';

test('only ?pwa=1 opts in through the URL', () => {
  assert.deepEqual(resolvePwaOptIn('1', false, false), { enable: true, clearStored: false });
  assert.deepEqual(resolvePwaOptIn('true', false, false), { enable: false, clearStored: false });
  assert.deepEqual(resolvePwaOptIn('', false, false), { enable: false, clearStored: false });
  assert.deepEqual(resolvePwaOptIn(null, false, false), { enable: false, clearStored: false });
});

test('?pwa=0 opts out and clears a stored opt-in', () => {
  assert.deepEqual(resolvePwaOptIn('0', true, false), { enable: false, clearStored: true });
  assert.deepEqual(resolvePwaOptIn('false', true, false), { enable: false, clearStored: true });
});

test('a stored opt-in or an installed standalone window keeps the worker', () => {
  assert.deepEqual(resolvePwaOptIn(null, true, false), { enable: true, clearStored: false });
  assert.deepEqual(resolvePwaOptIn(null, false, true), { enable: true, clearStored: false });
});
