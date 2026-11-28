import assert from 'node:assert/strict';
import { test } from 'node:test';
import { folderToSave, shownFolder } from './policyFolderDraft.ts';

test('leaving the folder field without a change saves nothing', () => {
  assert.equal(folderToSave(null, '.agents'), null);
  assert.equal(folderToSave('.agents', '.agents'), null);
  assert.equal(folderToSave('  .agents ', '.agents'), null);
});

test('an empty folder name is never saved', () => {
  assert.equal(folderToSave('   ', '.agents'), null);
});

test('a changed folder name is saved trimmed', () => {
  assert.equal(folderToSave(' policy ', '.agents'), 'policy');
  assert.equal(folderToSave('policy', undefined), 'policy');
});

test('a live reload does not replace the folder name being typed', () => {
  assert.equal(shownFolder('pol', '.agents'), 'pol');
  assert.equal(shownFolder(null, 'policy'), 'policy');
  assert.equal(shownFolder(null, undefined), '.agents');
});
