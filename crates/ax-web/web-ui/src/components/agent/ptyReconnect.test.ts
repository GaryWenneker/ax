import assert from 'node:assert/strict';
import test from 'node:test';

import { PTY_MAX_RETRIES, PTY_STABLE_MS, ptyClosed, ptyLink, ptyOpened, ptyServerError } from './ptyReconnect.ts';

test('a socket that opens and closes at once stops after the retry limit', () => {
  let link = ptyLink();
  let now = 0;
  const attempts: number[] = [];
  for (let cycle = 0; cycle < 20; cycle += 1) {
    link = ptyOpened(link, now);
    now += 50;
    const result = ptyClosed(link, now);
    link = result.link;
    if (result.next.kind === 'stop') break;
    attempts.push(result.next.attempt);
    now += result.next.delay;
  }
  assert.deepEqual(attempts, [1, 2, 3, 4, 5]);
  assert.equal(attempts.length, PTY_MAX_RETRIES);
});

test('a socket that never opens also stops after the retry limit', () => {
  let link = ptyLink();
  let stops = 0;
  for (let cycle = 0; cycle < 10; cycle += 1) {
    const result = ptyClosed(link, cycle * 1000);
    link = result.link;
    if (result.next.kind === 'stop') { stops += 1; break; }
  }
  assert.equal(stops, 1);
  assert.equal(link.retries, PTY_MAX_RETRIES);
});

test('a connection that stayed up resets the retry count', () => {
  let link = { ...ptyLink(), retries: 4 };
  link = ptyOpened(link, 1000);
  const result = ptyClosed(link, 1000 + PTY_STABLE_MS);
  assert.deepEqual(result.next, { kind: 'retry', attempt: 1, delay: 1000 });
});

test('an error frame from the server stops reconnecting', () => {
  let link = ptyOpened(ptyLink(), 0);
  link = ptyServerError(link);
  const result = ptyClosed(link, PTY_STABLE_MS * 10);
  assert.deepEqual(result.next, { kind: 'stop', reason: 'server-error' });
});

test('the retry delay doubles and is capped at ten seconds', () => {
  const delays = [1, 2, 3, 4, 5].map(retries => {
    const result = ptyClosed({ ...ptyLink(), retries: retries - 1 }, 0);
    return result.next.kind === 'retry' ? result.next.delay : -1;
  });
  assert.deepEqual(delays, [1000, 2000, 4000, 8000, 10_000]);
});
