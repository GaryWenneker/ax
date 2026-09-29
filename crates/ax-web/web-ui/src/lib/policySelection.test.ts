import assert from 'node:assert/strict';
import { test } from 'node:test';
import { nextRowSelection, resolveOpenTarget } from './policySelection.ts';

const ROWS = [
  { key: 'both', origin: 'project' },
  { key: 'both', origin: 'global', projectId: 2 },
  { key: 'global-only', origin: 'global', projectId: 2 },
  { key: 'local' },
];

test('L1 the route origin and projectId win', () => {
  assert.deepEqual(resolveOpenTarget(ROWS, 'both', 'global', 7), { origin: 'global', projectId: 7 });
});

test('L1 a global route without projectId takes it from the global row', () => {
  assert.deepEqual(resolveOpenTarget(ROWS, 'global-only', 'global', null), { origin: 'global', projectId: 2 });
});

test('L2 a project route opens the project copy', () => {
  assert.deepEqual(resolveOpenTarget(ROWS, 'both', 'project', null), {});
});

test('L3 a bare name prefers the project copy, else the global one', () => {
  assert.deepEqual(resolveOpenTarget(ROWS, 'both', null, null), {});
  assert.deepEqual(resolveOpenTarget(ROWS, 'local', null, null), {});
  assert.deepEqual(resolveOpenTarget(ROWS, 'global-only', null, null), { origin: 'global', projectId: 2 });
});

test('L3 an unknown name opens as a project item', () => {
  assert.deepEqual(resolveOpenTarget(ROWS, 'nope', null, null), {});
});

const visibleIds = ['a', 'b', 'c'];
const plain = { visibleIds, metaKey: false, shiftKey: false };

test('a plain click selects and opens the row', () => {
  const r = nextRowSelection({ ...plain, id: 'b', selected: new Set(), anchor: null });
  assert.deepEqual([...r.selected], ['b']);
  assert.equal(r.openId, 'b');
});

test('a plain click on the only selected row deselects it', () => {
  const r = nextRowSelection({ ...plain, id: 'b', selected: new Set(['b']), anchor: 'b' });
  assert.equal(r.selected.size, 0);
  assert.equal(r.openId, null);
});

test('a plain click on another row switches to it', () => {
  const r = nextRowSelection({ ...plain, id: 'c', selected: new Set(['b']), anchor: 'b' });
  assert.deepEqual([...r.selected], ['c']);
  assert.equal(r.openId, 'c');
});

test('a plain click inside a multi-selection narrows to that row', () => {
  const r = nextRowSelection({ ...plain, id: 'b', selected: new Set(['a', 'b']), anchor: 'a' });
  assert.deepEqual([...r.selected], ['b']);
  assert.equal(r.openId, 'b');
});
