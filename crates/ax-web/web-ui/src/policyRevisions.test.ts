import { test } from 'node:test';
import assert from 'node:assert/strict';
import { revisionHashPrefix, revisionSourceLabel } from './policyRevisions.ts';

test('revisionSourceLabel maps save and restore', () => {
  assert.equal(revisionSourceLabel('save'), 'Save');
  assert.equal(revisionSourceLabel('restore'), 'Package restore');
  assert.equal(revisionSourceLabel('other'), 'other');
});

test('revisionHashPrefix is twelve hex chars', () => {
  assert.equal(revisionHashPrefix('abcdef0123456789'), 'abcdef012345');
});
