import assert from 'node:assert/strict';
import { test } from 'node:test';
import { dropPolicyRow, indexPolicyRows, patchPolicyRow, policyRowKey } from './policyRowKeys.ts';

type Rule = { id: string; origin?: string; projectId?: number; rowKey?: string; enabled?: boolean };
const idOf = (r: Rule) => r.id;

// A project rule `foo` and the global.db copy of `foo` from another project.
const local: Rule = { id: 'foo', origin: 'project' };
const copy: Rule = { id: 'foo', origin: 'global', projectId: 7, rowKey: 'gr:7:foo' };
const rows = [local, copy];

test('a project rule and a global copy with the same id keep separate keys', () => {
  assert.equal(policyRowKey(local, idOf), 'foo');
  assert.equal(policyRowKey(copy, idOf), 'gr:7:foo');
  const byKey = indexPolicyRows(rows, idOf);
  assert.equal(byKey.get('foo'), local, 'the project row is not shadowed by the global copy');
  assert.equal(byKey.get('gr:7:foo'), copy);
});

test('toggling or deleting one row leaves its namesake alone', () => {
  const patched = patchPolicyRow(rows, 'foo', idOf, (r) => ({ ...r, enabled: false }));
  assert.equal(patched[0].enabled, false);
  assert.equal(patched[1].enabled, undefined);
  assert.deepEqual(dropPolicyRow(rows, 'foo', idOf), [copy]);
  assert.deepEqual(dropPolicyRow(rows, 'gr:7:foo', idOf), [local]);
});
