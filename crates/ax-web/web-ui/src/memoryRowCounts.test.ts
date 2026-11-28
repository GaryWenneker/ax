import assert from 'node:assert/strict';
import { test } from 'node:test';
import { memoryRowCounts } from './memoryHeadline.ts';

const commits = '\n\nCommits:\n- abc1234 First\n- def5678 Second';

test('files and commits are both counted', () => {
  assert.equal(memoryRowCounts({ files: ['a', 'b'], body: `Fix${commits}` }), '2 files · 2 commits');
});

test('singular forms and missing parts', () => {
  assert.equal(memoryRowCounts({ files: ['a'], body: 'Fix' }), '1 file');
  assert.equal(memoryRowCounts({ files: [], body: 'Fix\n\nCommits:\n- abc1234 One' }), '1 commit');
  assert.equal(memoryRowCounts({ files: [], body: 'Fix' }), '');
});

test('commit-like lines in the outcome are not commits', () => {
  const body = 'Fix\n\nChanges:\nM a.rs\n\nOutcome: see\n- abc1234 not a commit';
  assert.equal(memoryRowCounts({ files: ['a.rs'], body }), '1 file');
});

test('a Commits heading quoted in the outcome is not counted', () => {
  const body = 'Fix\n\nOutcome: I ran\n\nCommits:\n- abc1234 quoted';
  assert.equal(memoryRowCounts({ files: [], body }), '');
});

test('the commits section ends at the next section', () => {
  const body = `Fix${commits}\n\nChanges:\nM a.rs\n- 1234567 x`;
  assert.equal(memoryRowCounts({ files: ['a.rs'], body }), '1 file · 2 commits');
});

test('a body with CRLF line endings counts its commits like the detail view', () => {
  const body = 'Fix\r\n\r\nCommits:\r\n- abc1234 First\r\n- def5678 Second\r\n\r\nOutcome: done';
  assert.equal(memoryRowCounts({ files: [], body }), '2 commits');
});

test('the commit count agrees with splitMemoryBody for an outcome without a space', () => {
  const body = 'Fix\n\nOutcome:\nI ran\n\nCommits:\n- abc1234 quoted';
  assert.equal(memoryRowCounts({ files: [], body }), '');
});
