import assert from 'node:assert/strict';
import { test } from 'node:test';
import { waitForSignIn, type SignInStatus } from './signIn.ts';

function harness(answers: (SignInStatus | Error)[], completeDelay = 0) {
  const log: string[] = [];
  let inFlight = 0;
  let maxInFlight = 0;
  let stopped = false;
  const track = async <T,>(name: string, work: () => Promise<T>): Promise<T> => {
    inFlight += 1;
    maxInFlight = Math.max(maxInFlight, inFlight);
    log.push(name);
    try {
      return await work();
    } finally {
      inFlight -= 1;
    }
  };
  return {
    log,
    max: () => maxInFlight,
    stop: () => { stopped = true; },
    steps: {
      status: () => track('status', async () => {
        await new Promise((r) => setTimeout(r, 5));
        const next = answers.shift() ?? { state: 'pending' as const };
        if (next instanceof Error) throw next;
        return next;
      }),
      complete: () => track('complete', async () => {
        await new Promise((r) => setTimeout(r, completeDelay));
        return { state: 'connected' as const, connection: 'saved' };
      }),
      pause: async () => { log.push('pause'); },
      stopped: () => stopped,
    },
  };
}

test('it asks again until the sign-in is ready, then completes exactly once', async () => {
  const h = harness([{ state: 'pending' }, { state: 'pending' }, { state: 'ready' }]);
  assert.deepEqual(await waitForSignIn(h.steps), { state: 'connected', connection: 'saved' });
  assert.deepEqual(h.log, ['status', 'pause', 'status', 'pause', 'status', 'complete']);
});

test('a slow completion is never overtaken by another request', async () => {
  // Completing takes far longer than the pause between questions.
  const h = harness([{ state: 'ready' }], 80);
  assert.deepEqual(await waitForSignIn(h.steps), { state: 'connected', connection: 'saved' });
  assert.equal(h.max(), 1, 'one request at a time');
  assert.deepEqual(h.log, ['status', 'complete']);
});

test('a failed sign-in and a failed request both end with their message', async () => {
  const denied = harness([{ state: 'pending' }, { state: 'failed', message: 'Access was denied' }]);
  assert.deepEqual(await waitForSignIn(denied.steps), { state: 'failed', message: 'Access was denied' });
  assert.ok(!denied.log.includes('complete'));
  const broken = harness([new Error('ax-admin is not running.')]);
  assert.deepEqual(await waitForSignIn(broken.steps), { state: 'failed', message: 'ax-admin is not running.' });
});

test('once stopped it asks nothing more and saves nothing', async () => {
  const h = harness([{ state: 'pending' }, { state: 'ready' }]);
  h.steps.pause = async () => { h.log.push('pause'); h.stop(); };
  assert.deepEqual(await waitForSignIn(h.steps), { state: 'stopped', saved: null });
  assert.deepEqual(h.log, ['status', 'pause']);
  // Stopped while a question was in flight: its "ready" answer is not acted on.
  const late = harness([{ state: 'ready' }]);
  const outcome = waitForSignIn(late.steps);
  late.stop();
  assert.deepEqual(await outcome, { state: 'stopped', saved: null });
  assert.ok(!late.log.includes('complete'));
});

test('stopped while the connection is being saved: the saved connection is handed back, not announced', async () => {
  const h = harness([{ state: 'ready' }], 40);
  const outcome = waitForSignIn(h.steps);
  // The user cancels during the slow completion.
  setTimeout(() => h.stop(), 20);
  assert.deepEqual(await outcome, { state: 'stopped', saved: 'saved' });
  assert.deepEqual(h.log, ['status', 'complete']);
});

test('stopped while a completion fails is still just stopped', async () => {
  const h = harness([{ state: 'ready' }]);
  h.steps.complete = async () => { h.stop(); throw new Error('lost'); };
  assert.deepEqual(await waitForSignIn(h.steps), { state: 'stopped', saved: null });
});
