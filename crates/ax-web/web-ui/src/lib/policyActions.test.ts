import assert from 'node:assert/strict';
import { test } from 'node:test';
import { runThenReload } from './policyActions.ts';

test('a failed policy action shows its error and still reloads the list', async () => {
  const errors: string[] = [];
  let reloads = 0;
  await runThenReload(
    () => Promise.reject(new Error('403 read-only share')),
    () => { reloads += 1; },
    (message) => errors.push(message),
    'Delete failed',
  );
  assert.deepEqual(errors, ['403 read-only share']);
  assert.equal(reloads, 1);
});

test('a failure that is not an Error falls back to the given message', async () => {
  const errors: string[] = [];
  await runThenReload(() => Promise.reject('nope'), () => {}, (m) => errors.push(m), 'Delete failed');
  assert.deepEqual(errors, ['Delete failed']);
});

test('a successful policy action reloads once and reports no error', async () => {
  const errors: string[] = [];
  let reloads = 0;
  await runThenReload(() => Promise.resolve(), () => { reloads += 1; }, (m) => errors.push(m), 'Delete failed');
  assert.deepEqual(errors, []);
  assert.equal(reloads, 1);
});
