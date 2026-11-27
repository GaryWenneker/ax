import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdirOutcome, switchOutcome } from './workspaceSwitch.ts';

test('a switch the server confirms is a switch, with or without the switched flag', () => {
  assert.deepEqual(switchOutcome({ ok: true, switched: true, path: '/work/app' }), { kind: 'switched', path: '/work/app' });
  assert.deepEqual(switchOutcome({ ok: true, path: '/work/app' }), { kind: 'switched', path: '/work/app' });
});

test('a reloading switch reloads the page', () => {
  assert.deepEqual(switchOutcome({ ok: true, reloading: true }), { kind: 'reload' });
});

test('a folder that is not an ax project asks for init and browses there', () => {
  assert.deepEqual(switchOutcome({ ok: false, needs_init: true, error: 'Project not initialized' }, '/work/new'), {
    kind: 'needs_init',
    path: '/work/new',
    message: 'Not an ax project yet — use Initialize below.',
  });
});

test('a failed switch shows the server message or a fallback', () => {
  assert.deepEqual(switchOutcome({ ok: false, error: 'Path not allowed' }), { kind: 'error', message: 'Path not allowed' });
  assert.deepEqual(switchOutcome({ ok: false }), { kind: 'error', message: 'Switch failed' });
});

test('a created folder is browsed next; a failure keeps the shown folder', () => {
  assert.deepEqual(mkdirOutcome({ ok: true, path: '/work/new' }, '/work'), { browse: '/work/new', error: null });
  assert.deepEqual(mkdirOutcome({ ok: true }, '/work'), { browse: '/work', error: null });
  assert.deepEqual(mkdirOutcome({ ok: false, error: 'exists' }, '/work'), { browse: null, error: 'exists' });
  assert.deepEqual(mkdirOutcome({ ok: false }, '/work'), { browse: null, error: 'Create failed' });
});
