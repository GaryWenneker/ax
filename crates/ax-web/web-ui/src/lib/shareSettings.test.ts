import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { saveQuietly, shareFieldDisabled, startSerialPoll } from './shareSettings.ts';

function fakeTimers() {
  const timers = new Map<number, () => void>();
  let next = 1;
  return {
    set: (cb: () => void) => { const id = next++; timers.set(id, cb); return id; },
    clear: (id: number) => { timers.delete(id); },
    fire: () => { const cbs = [...timers.values()]; timers.clear(); cbs.forEach(cb => cb()); },
    count: () => timers.size,
  };
}

const settle = () => new Promise<void>(resolve => setImmediate(resolve));

describe('startSerialPoll', () => {
  it('waits for a slow poll to settle before scheduling the next one', async () => {
    const timers = fakeTimers();
    let inFlight = 0;
    let maxInFlight = 0;
    let completions = 0;
    const resolvers: Array<(done: boolean) => void> = [];
    startSerialPoll({
      intervalMs: 5000,
      tick: () => {
        inFlight += 1;
        maxInFlight = Math.max(maxInFlight, inFlight);
        return new Promise<boolean>(resolve => resolvers.push((done) => { inFlight -= 1; if (done) completions += 1; resolve(done); }));
      },
      setTimer: timers.set,
      clearTimer: timers.clear,
    });
    timers.fire();
    // The interval elapses again while the first poll is still running.
    timers.fire();
    timers.fire();
    assert.equal(resolvers.length, 1);
    resolvers[0](false);
    await settle();
    assert.equal(timers.count(), 1);
    timers.fire();
    assert.equal(resolvers.length, 2);
    resolvers[1](true);
    await settle();
    assert.equal(timers.count(), 0);
    assert.equal(maxInFlight, 1);
    assert.equal(completions, 1);
  });

  it('stops without another poll after stop() or a failed poll', async () => {
    const timers = fakeTimers();
    let polls = 0;
    const poller = startSerialPoll({ intervalMs: 10, tick: async () => { polls += 1; return false; }, setTimer: timers.set, clearTimer: timers.clear });
    poller.stop();
    timers.fire();
    assert.equal(polls, 0);
    const failing = startSerialPoll({ intervalMs: 10, tick: async () => { polls += 1; throw new Error('down'); }, setTimer: timers.set, clearTimer: timers.clear });
    timers.fire();
    await settle();
    assert.equal(polls, 1);
    assert.equal(timers.count(), 0);
    failing.stop();
  });
});

describe('saveQuietly', () => {
  it('reports a failed save instead of rejecting', async () => {
    const errors: string[] = [];
    const ok = await saveQuietly(async () => { throw new Error('server unreachable'); }, (m) => errors.push(m));
    assert.equal(ok, false);
    assert.deepEqual(errors, ['server unreachable']);
    assert.equal(await saveQuietly(async () => undefined, (m) => errors.push(m)), true);
  });
});

describe('shareFieldDisabled', () => {
  it('keeps text fields usable while a blur save runs so focus is not dropped', () => {
    assert.equal(shareFieldDisabled({ loaded: true, busy: 'save', kind: 'text' }), false);
    assert.equal(shareFieldDisabled({ loaded: true, busy: 'save', kind: 'control' }), true);
    assert.equal(shareFieldDisabled({ loaded: true, busy: 'sync', kind: 'text' }), true);
    assert.equal(shareFieldDisabled({ loaded: false, busy: null, kind: 'text' }), true);
    assert.equal(shareFieldDisabled({ loaded: true, busy: null, kind: 'control' }), false);
  });
});
