import assert from 'node:assert/strict';
import { test } from 'node:test';
import { trackNewKeys, sparkPhase, edgeMarker, SPARK_MS, createDebouncer, diffNewKeys, parseChange, planSparks, type Scheduler } from './live.ts';

test('first load never marks anything new', () => {
  assert.deepEqual(diffNewKeys(null, ['a', 'b']), []);
});

test('only keys absent before are new; removed keys are not', () => {
  assert.deepEqual(diffNewKeys(['a', 'x'], ['a', 'b']), ['b']);
  assert.deepEqual(diffNewKeys(['a', 'b'], ['a']), []);
});

test('parseChange accepts a known topic and rejects the rest', () => {
  assert.deepEqual(parseChange('{"topic":"memory","version":"ab"}'), { topic: 'memory', version: 'ab' });
  assert.equal(parseChange('{"topic":"nope","version":"ab"}'), null);
  assert.equal(parseChange('not json'), null);
});

function fakeScheduler() {
  let now = 0;
  const timers = new Map<number, { at: number; fn: () => void }>();
  let next = 1;
  const sched: Scheduler = {
    set: (fn, ms) => {
      const id = next++;
      timers.set(id, { at: now + ms, fn });
      return id;
    },
    clear: (h) => {
      timers.delete(h as number);
    },
  };
  const advance = (ms: number) => {
    now += ms;
    for (const [id, t] of [...timers]) {
      if (t.at <= now) {
        timers.delete(id);
        t.fn();
      }
    }
  };
  return { sched, advance };
}

test('five triggers within 400 ms cause exactly one reload', () => {
  const { sched, advance } = fakeScheduler();
  let calls = 0;
  const trigger = createDebouncer(400, () => calls++, sched);
  for (let i = 0; i < 5; i++) {
    trigger();
    advance(50);
  }
  assert.equal(calls, 0);
  advance(400);
  assert.equal(calls, 1);
});

test('spark cap: 50 new nodes give 20 sparks and one summary', () => {
  const ids = Array.from({ length: 50 }, (_, i) => `n${i}`);
  const plan = planSparks(ids);
  assert.equal(plan.spark.length, 20);
  assert.equal(plan.summary, 50);
});

test('a few new nodes all spark without a summary', () => {
  assert.deepEqual(planSparks(['a', 'b']), { spark: ['a', 'b'], summary: null });
});

test('a spark has two growing rings and fades out after 1.2 s', () => {
  const start = sparkPhase(0);
  assert.ok(start);
  assert.equal(start.rings.length, 2);
  assert.equal(start.rings[0].grow, 0);
  assert.ok(start.core > 0.9);
  const mid = sparkPhase(600)!;
  assert.ok(mid.rings[0].grow > mid.rings[1].grow);
  assert.ok(mid.rings[0].alpha < start.rings[0].alpha);
  assert.equal(sparkPhase(SPARK_MS + 1), null);
});

test('an on-screen node gets no edge marker', () => {
  assert.equal(edgeMarker(100, 100, 800, 600, 12), null);
});

test('an off-screen node gets a marker clamped to the edge, pointing at it', () => {
  const m = edgeMarker(1200, 300, 800, 600, 12)!;
  assert.equal(m.x, 788);
  assert.equal(m.y, 300);
  assert.ok(Math.abs(m.angle) < 1e-9);
  const up = edgeMarker(400, -50, 800, 600, 12)!;
  assert.equal(up.y, 12);
  assert.ok(Math.abs(up.angle + Math.PI / 2) < 1e-9);
});

test('a key that appears within the same scope is new', () => {
  const first = trackNewKeys(null, 'q=a', ['1']);
  assert.deepEqual(first.added, []);
  const next = trackNewKeys(first.state, 'q=a', ['1', '2']);
  assert.deepEqual(next.added, ['2']);
});

test('a scope change (filter, page, search) resets the baseline and marks nothing', () => {
  const first = trackNewKeys(null, 'q=a', ['1']);
  const other = trackNewKeys(first.state, 'q=b', ['7', '8']);
  assert.deepEqual(other.added, []);
  assert.deepEqual(trackNewKeys(other.state, 'q=b', ['7', '8', '9']).added, ['9']);
});
