import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { chromeAfterTrashClick } from './projectSwitchChrome.ts';

test('trash click closes Switch project and keeps the chosen project for the dialog', () => {
  const project = { path: '/Users/gary/io/ax-ctxcache', label: 'ax-ctxcache' };
  const next = chromeAfterTrashClick(project);
  assert.equal(next.menuOpen, false);
  assert.equal(next.project, project);
});

test('the header switcher applies that closed menu and hides the list while the dialog is open', () => {
  const source = readFileSync(new URL('./LoggingProjectSwitch.tsx', import.meta.url), 'utf8');
  assert.match(source, /const next = chromeAfterTrashClick\(p\)/);
  assert.match(source, /setOpen\(next\.menuOpen\)/);
  assert.match(source, /setPurge\(next\.project\)/);
  assert.match(source, /const menuShown = open && purge == null/);
});
