import assert from 'node:assert/strict';
import { test } from 'node:test';
import { joinPolicyList, policyEditorCanSave, policyEditorView } from './policyEditorLoad.ts';

test('a list missing from loaded frontmatter shows as an empty field instead of throwing', () => {
  assert.equal(joinPolicyList(undefined), '');
  assert.equal(joinPolicyList(null), '');
  assert.equal(joinPolicyList(['**/*.ts', 'deploy']), '**/*.ts, deploy');
});

test('an existing rule or skill that failed to load can not be saved over', () => {
  assert.equal(policyEditorCanSave({ load: 'failed', saving: false }), false);
  assert.equal(policyEditorCanSave({ load: 'loading', saving: false }), false);
  assert.equal(policyEditorCanSave({ load: 'ready', saving: true }), false);
  assert.equal(policyEditorCanSave({ load: 'ready', saving: false }), true);
});

test('a failed load replaces the form with a retry view', () => {
  assert.equal(policyEditorView('failed'), 'retry');
  assert.equal(policyEditorView('loading'), 'spinner');
  assert.equal(policyEditorView('ready'), 'form');
});
