import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  groupErrors,
  installableServers,
  parseProgress,
  progressLabel,
  progressPercent,
} from './lspServers.ts';

test('only unavailable servers with a recipe are installable, in list order', () => {
  const ids = installableServers([
    { id: 'rust-analyzer', available: true, install: 'rustup component add rust-analyzer' },
    { id: 'pyright', available: false, install: 'npm i -g pyright' },
    { id: 'odd', available: false },
    { id: 'blank', available: false, install: '  ' },
    { id: 'gopls', available: false, path: '/x/gopls', install: 'go install gopls' },
  ]).map((s) => s.id);
  assert.deepEqual(ids, ['pyright', 'gopls']);
});

test('progress percent is whole, 0 without a total, and capped at 100', () => {
  assert.equal(progressPercent(0, 0), 0);
  assert.equal(progressPercent(5, 0), 0);
  assert.equal(progressPercent(1, 3), 33);
  assert.equal(progressPercent(2, 3), 66);
  assert.equal(progressPercent(3, 3), 100);
  assert.equal(progressPercent(7, 3), 100);
});

test('progress label shows done of total with the percent', () => {
  assert.equal(progressLabel({ done: 120, total: 999 }), '120 / 999 · 12%');
  assert.equal(progressLabel({ done: 0, total: 0 }), '0 / 0 · 0%');
  assert.equal(progressLabel({ done: 1234, total: 5000 }), '1,234 / 5,000 · 24%');
});

test('progress JSON is accepted only when it has the snapshot shape', () => {
  const ok = { running: true, done: 3, total: 9, server: 'gopls', file: 'a.go', cancelled: false };
  assert.deepEqual(parseProgress(ok), ok);
  assert.deepEqual(parseProgress({ ...ok, server: null, file: null }), {
    ...ok,
    server: null,
    file: null,
  });
  assert.equal(parseProgress(null), null);
  assert.equal(parseProgress({ error: 'nope' }), null);
  assert.equal(parseProgress({ ...ok, done: '3' }), null);
  assert.equal(parseProgress({ ...ok, running: 'yes' }), null);
  assert.equal(parseProgress({ ...ok, server: 7 }), null);
});

test('errors are grouped by message in the order messages first appear', () => {
  assert.deepEqual(groupErrors(['a.ts: X', 'c.cs: Y', 'b.ts: X']), [
    { message: 'X', files: ['a.ts', 'b.ts'] },
    { message: 'Y', files: ['c.cs'] },
  ]);
  assert.deepEqual(groupErrors([]), []);
});

test('an error splits at its first ": " only, and text without a file has no files', () => {
  assert.deepEqual(groupErrors(['a.ts: LSP error: {"code":1}']), [
    { message: 'LSP error: {"code":1}', files: ['a.ts'] },
  ]);
  assert.deepEqual(groupErrors(['cancelled']), [{ message: 'cancelled', files: [] }]);
});

test('a file repeated under one message is listed once', () => {
  assert.deepEqual(groupErrors(['a.ts: X', 'a.ts: X']), [{ message: 'X', files: ['a.ts'] }]);
});
