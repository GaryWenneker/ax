import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createLatestSaver } from './latestSaver.ts';

function deferred() {
  let resolve!: () => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<void>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

test('a drag of many steps sends one write with the last value', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const sent: number[] = [];
  const saved: number[] = [];
  const saver = createLatestSaver<number>(async (v) => { sent.push(v); }, {
    delayMs: 300,
    onSaved: (v) => saved.push(v),
    onError: () => assert.fail('no error expected'),
  });
  for (let v = 8000; v <= 40000; v += 1000) saver.set(v);
  t.mock.timers.tick(300);
  t.mock.timers.reset();
  await tick();
  assert.deepEqual(sent, [40000]);
  assert.deepEqual(saved, [40000]);
});

test('a value set while a write is in flight is sent after it, never alongside it', async () => {
  const writes: { value: number; done: ReturnType<typeof deferred> }[] = [];
  const saved: number[] = [];
  const saver = createLatestSaver<number>((value) => {
    const done = deferred();
    writes.push({ value, done });
    return done.promise;
  }, { delayMs: 0, onSaved: (v) => saved.push(v), onError: () => undefined });
  saver.set(1000);
  saver.flush();
  saver.set(2000);
  saver.flush();
  assert.equal(writes.length, 1, 'the second write waits for the first');
  writes[0].done.resolve();
  await tick();
  assert.deepEqual(writes.map((w) => w.value), [1000, 2000]);
  assert.deepEqual(saved, [], 'the superseded write reports nothing');
  writes[1].done.resolve();
  await tick();
  assert.deepEqual(saved, [2000]);
});

test('only the newest value reports a failure', async () => {
  const errors: unknown[] = [];
  const saver = createLatestSaver<number>(async () => { throw new Error('502'); }, {
    delayMs: 0,
    onSaved: () => assert.fail('no success expected'),
    onError: (e) => errors.push(e),
  });
  saver.set(1000);
  saver.flush();
  await tick();
  assert.equal(errors.length, 1);
});

test('dispose sends a value that is still waiting, but reports nothing', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const sent: number[] = [];
  const saver = createLatestSaver<number>(async (v) => { sent.push(v); }, {
    delayMs: 400,
    onSaved: () => assert.fail('a disposed saver reports nothing'),
    onError: () => assert.fail('a disposed saver reports nothing'),
  });
  saver.set(12000);
  saver.dispose();
  t.mock.timers.reset();
  await tick();
  assert.deepEqual(sent, [12000]);
});

test('dispose while a write is in flight still sends the newer value afterwards', async () => {
  const writes: { value: number; done: ReturnType<typeof deferred> }[] = [];
  const saver = createLatestSaver<number>((value) => {
    const done = deferred();
    writes.push({ value, done });
    return done.promise;
  }, { delayMs: 400, onSaved: () => assert.fail('no report after dispose'), onError: () => undefined });
  saver.set(1000);
  saver.flush();
  saver.set(2000);
  saver.dispose();
  assert.equal(writes.length, 1);
  writes[0].done.resolve();
  await tick();
  assert.deepEqual(writes.map((w) => w.value), [1000, 2000]);
  writes[1].done.resolve();
  await tick();
});

test('a value set after dispose is ignored', async () => {
  const sent: number[] = [];
  const saver = createLatestSaver<number>(async (v) => { sent.push(v); }, { delayMs: 0, onSaved: () => undefined, onError: () => undefined });
  saver.dispose();
  saver.set(5000);
  saver.flush();
  await tick();
  assert.deepEqual(sent, []);
});
