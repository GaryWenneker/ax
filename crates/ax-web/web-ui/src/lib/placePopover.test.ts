import assert from 'node:assert/strict';
import { test } from 'node:test';
import { followViewport } from './placePopover.ts';

type Listener = { type: string; fn: () => void; options: unknown };

function fakeTarget() {
  const listeners: Listener[] = [];
  return {
    listeners,
    addEventListener(type: string, fn: () => void, options?: unknown) {
      listeners.push({ type, fn, options });
    },
    removeEventListener(type: string, fn: () => void) {
      const i = listeners.findIndex((l) => l.type === type && l.fn === fn);
      if (i >= 0) listeners.splice(i, 1);
    },
    fire(type: string) {
      for (const l of [...listeners]) if (l.type === type) l.fn();
    },
  };
}

test('a burst of scroll events re-places the popover once per frame', () => {
  const target = fakeTarget();
  const frames: (() => void)[] = [];
  let placed = 0;
  const stop = followViewport(() => { placed += 1; }, target, (cb) => frames.push(cb), () => {});
  for (let i = 0; i < 25; i += 1) target.fire('scroll');
  target.fire('resize');
  assert.equal(placed, 0, 'no synchronous layout work inside the scroll handler');
  assert.equal(frames.length, 1);
  frames.shift()!();
  assert.equal(placed, 1);
  target.fire('scroll');
  assert.equal(frames.length, 1);
  stop();
});

test('the scroll listener is passive and capturing, and cleanup removes it and cancels the frame', () => {
  const target = fakeTarget();
  const cancelled: number[] = [];
  const stop = followViewport(() => {}, target, () => 7, (id) => cancelled.push(id));
  const scroll = target.listeners.find((l) => l.type === 'scroll');
  assert.deepEqual(scroll?.options, { passive: true, capture: true });
  target.fire('scroll');
  stop();
  assert.deepEqual(cancelled, [7]);
  assert.equal(target.listeners.length, 0);
});
