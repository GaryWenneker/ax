import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browseOutcome, EMPTY_BROWSE_VIEW, type BrowseView } from './projectBrowse.ts';

const project: BrowseView = {
  path: '/work/app',
  parent: '/work',
  initialized: true,
  entries: [{ name: 'src', path: '/work/app/src', is_dir: true, initialized: false }],
};

test('a failed browse keeps the shown folder, its actions and its path together', () => {
  const out = browseOutcome(project, { ok: false, error: 'No such folder: /work/ap' }, true);
  assert.ok(out);
  assert.deepEqual(out.view, project);
  assert.equal(out.view.path, '/work/app', 'Switch acts on the folder that is shown, not the mistyped one');
  assert.equal(out.draft, '/work/app');
  assert.equal(out.error, 'No such folder: /work/ap');
});

test('a successful browse replaces folder, parent, flag and entries at once', () => {
  const out = browseOutcome(project, { ok: true, path: '/work/other/', parent: '/work', initialized: false, entries: [] }, true);
  assert.ok(out);
  assert.deepEqual(out.view, { path: '/work/other', parent: '/work', initialized: false, entries: [] });
  assert.equal(out.draft, '/work/other');
  assert.equal(out.error, null);
});

test('a response that is no longer the latest request is ignored', () => {
  assert.equal(browseOutcome(project, { ok: true, path: '/slow/earlier', entries: [] }, false), null);
  assert.equal(browseOutcome(project, { ok: false, error: 'late' }, false), null);
});

test('a response without a path counts as a failure', () => {
  const out = browseOutcome(EMPTY_BROWSE_VIEW, { ok: true }, true);
  assert.ok(out);
  assert.equal(out.error, 'Browse failed');
  assert.deepEqual(out.view, EMPTY_BROWSE_VIEW);
});
