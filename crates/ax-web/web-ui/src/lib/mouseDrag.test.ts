import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createMouseDrag } from './mouseDrag.ts';

function fakeClassList() {
  const set = new Set<string>();
  return { add: (c: string) => set.add(c), remove: (c: string) => set.delete(c), has: (c: string) => set.has(c) };
}

test('a drag applies the start value plus the pointer distance and marks the body while it runs', () => {
  const body = fakeClassList();
  const applied: number[] = [];
  const drag = createMouseDrag({ bodyClass: 'policy-list-resizing', classList: body, begin: () => 300, move: (start, dx) => applied.push(start + dx) });
  drag.move(500);
  assert.deepEqual(applied, [], 'moves before a press do nothing');
  drag.down(100);
  assert.ok(body.has('policy-list-resizing'));
  drag.move(140);
  drag.move(60);
  assert.deepEqual(applied, [340, 260]);
  drag.up();
  assert.ok(!body.has('policy-list-resizing'));
  drag.move(400);
  assert.deepEqual(applied, [340, 260], 'moves after release do nothing');
});

test('the start value is read again on each press', () => {
  const body = fakeClassList();
  let stored = 300;
  const applied: number[] = [];
  const drag = createMouseDrag({ bodyClass: 'x', classList: body, begin: () => stored, move: (start, dx) => { stored = start - dx; applied.push(stored); } });
  drag.down(100);
  drag.move(80);
  drag.up();
  drag.down(0);
  drag.move(10);
  assert.deepEqual(applied, [320, 310]);
  drag.dispose();
  assert.ok(!body.has('x'));
});
