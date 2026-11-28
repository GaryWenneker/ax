import assert from 'node:assert/strict';
import test from 'node:test';
import { installChunkReload, shouldReloadForChunkError } from './chunkReload.ts';

test('a missing chunk reloads once, then not again within the guard window', () => {
  assert.equal(shouldReloadForChunkError(null, 1_000), true);
  assert.equal(shouldReloadForChunkError(1_000, 5_000), false);
  assert.equal(shouldReloadForChunkError(1_000, 1_000 + 60_001), true);
  assert.equal(shouldReloadForChunkError(Number.NaN, 5_000), true);
});

test('a preload error after an upgrade reloads the page and stops the error', () => {
  const listeners = new Map<string, (event: { preventDefault(): void }) => void>();
  const store = new Map<string, string>();
  let reloads = 0;
  installChunkReload({
    addEventListener: (type, fn) => listeners.set(type, fn),
    storage: { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => void store.set(k, v) },
    reload: () => { reloads += 1; },
    now: () => 10_000,
  });
  let prevented = 0;
  const fire = () => listeners.get('vite:preloadError')?.({ preventDefault: () => { prevented += 1; } });
  fire();
  assert.equal(reloads, 1);
  assert.equal(prevented, 1);
  fire();
  assert.equal(reloads, 1, 'a second failure in the same window shows the error instead of looping');
  assert.equal(prevented, 1);
});
