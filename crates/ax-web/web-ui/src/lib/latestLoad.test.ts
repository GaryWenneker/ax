import assert from 'node:assert/strict';
import { test } from 'node:test';
import { applyLatest, createLatestGate, dataAfterFailedLoad } from './latestLoad.ts';

test('a failed background refresh keeps what is on screen; a failed user load clears it', () => {
  const shown = { total: 3 };
  assert.equal(dataAfterFailedLoad(shown, true), shown);
  assert.equal(dataAfterFailedLoad(shown, false), null);
});

test('only the newest request may apply its response', () => {
  const gate = createLatestGate();
  const first = gate.begin();
  const second = gate.begin();
  assert.equal(gate.isLatest(first), false);
  assert.equal(gate.isLatest(second), true);
});

function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

test('a slower answer for an earlier choice does not replace the newer one', async () => {
  const gate = createLatestGate();
  const shown: string[] = [];
  const a = deferred<string>();
  const b = deferred<string>();
  const first = applyLatest(gate, () => a.promise, { value: (v) => shown.push(v) });
  const second = applyLatest(gate, () => b.promise, { value: (v) => shown.push(v) });
  b.resolve('B');
  await second;
  a.resolve('A');
  assert.equal(await first, false);
  assert.deepEqual(shown, ['B']);
});

test('an answer that arrives after cancel is dropped, error and settled included', async () => {
  const gate = createLatestGate();
  const events: string[] = [];
  const a = deferred<string>();
  const run = applyLatest(gate, () => a.promise, {
    value: (v) => events.push(`value ${v}`),
    error: () => events.push('error'),
    settled: () => events.push('settled'),
  });
  gate.cancel();
  a.reject(new Error('late'));
  assert.equal(await run, false);
  assert.deepEqual(events, []);
});

test('the newest answer applies its value or error, then settles', async () => {
  const gate = createLatestGate();
  const events: string[] = [];
  const on = {
    value: (v: string) => events.push(`value ${v}`),
    error: (e: unknown) => events.push(`error ${(e as Error).message}`),
    settled: () => events.push('settled'),
  };
  assert.equal(await applyLatest(gate, async () => 'ok', on), true);
  assert.equal(await applyLatest(gate, async () => { throw new Error('boom'); }, on), true);
  assert.deepEqual(events, ['value ok', 'settled', 'error boom', 'settled']);
});
