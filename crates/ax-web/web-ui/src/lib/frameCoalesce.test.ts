import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { coalesceToFrame } from './frameCoalesce.ts';

function fakeFrames() {
  const queue = new Map<number, () => void>();
  let next = 1;
  return {
    schedule: (cb: () => void) => { const id = next++; queue.set(id, cb); return id; },
    cancel: (id: number) => { queue.delete(id); },
    flush: () => { const cbs = [...queue.values()]; queue.clear(); cbs.forEach(cb => cb()); },
    pending: () => queue.size,
  };
}

describe('coalesceToFrame', () => {
  it('runs once per frame however many DOM changes arrive', () => {
    const frames = fakeFrames();
    let runs = 0;
    const update = coalesceToFrame(() => { runs += 1; }, frames.schedule, frames.cancel);
    for (let i = 0; i < 50; i += 1) update.request();
    assert.equal(runs, 0);
    assert.equal(frames.pending(), 1);
    frames.flush();
    assert.equal(runs, 1);
    update.request();
    frames.flush();
    assert.equal(runs, 2);
  });
  it('drops a pending run when cancelled', () => {
    const frames = fakeFrames();
    let runs = 0;
    const update = coalesceToFrame(() => { runs += 1; }, frames.schedule, frames.cancel);
    update.request();
    update.cancel();
    frames.flush();
    assert.equal(runs, 0);
  });
});
