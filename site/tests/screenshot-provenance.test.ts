import test from 'node:test';
import assert from 'node:assert/strict';
import { screenshotEntryIssues } from '../scripts/screenshot-provenance.mjs';

test('a manifest entry without state or data is reported, not a crash', () => {
  const issues = screenshotEntryIssues('new.png', { file: 'new.png', imageHash: 'h', sourceHash: 's' }, 'h');
  assert.deepEqual(issues, ['screenshot source/state missing: new.png']);
});

test('a non-string data field is reported, not a crash', () => {
  const issues = screenshotEntryIssues('x.png', { file: 'x.png', imageHash: 'h', sourceHash: 's', data: 42 }, 'h');
  assert.deepEqual(issues, ['screenshot source/state missing: x.png']);
});

test('entries with state or public data pass', () => {
  assert.deepEqual(screenshotEntryIssues('a.png', { imageHash: 'h', sourceHash: 's', state: 'seeded' }, 'h'), []);
  assert.deepEqual(screenshotEntryIssues('b.png', { imageHash: 'h', sourceHash: 's', data: 'public site' }, 'h'), []);
});

test('missing entry and stale hash are reported', () => {
  assert.deepEqual(screenshotEntryIssues('c.png', undefined, 'h'), ['screenshot provenance missing: c.png']);
  assert.deepEqual(screenshotEntryIssues('d.png', { imageHash: 'old', sourceHash: 's', state: 'x' }, 'new'), ['screenshot hash missing/stale: d.png']);
});
