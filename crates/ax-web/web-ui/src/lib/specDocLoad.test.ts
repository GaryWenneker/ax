import assert from 'node:assert/strict';
import { test } from 'node:test';
import { editorResetForLoad, specDocKey } from './specDocLoad.ts';

test('opening another document clears the previous text, version and revisions first', () => {
  const reset = editorResetForLoad(specDocKey('spec', 'A'), specDocKey('spec', 'B'));
  assert.deepEqual(reset, { revisions: [], openVersion: null, draftText: '', dirty: false });
});

test('a plan and a spec with the same id are different documents', () => {
  assert.notEqual(editorResetForLoad(specDocKey('spec', 'x'), specDocKey('plan', 'x')), null);
});

test('reloading the open document after a save or approval keeps what is shown', () => {
  assert.equal(editorResetForLoad(specDocKey('spec', 'A'), specDocKey('spec', 'A')), null);
});

test('the first load starts from an empty editor', () => {
  assert.notEqual(editorResetForLoad(null, specDocKey('spec', 'A')), null);
});
