import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('./RemoteHostSetup.tsx', import.meta.url), 'utf8');

test('a failed poll is kept apart from an action error', () => {
  const effect = source.slice(source.indexOf('useEffect(() => {'), source.indexOf('async function act('));
  assert.ok(effect.length > 0);
  assert.doesNotMatch(effect, /setError\(/, 'polling must not write the action error');
  assert.match(effect, /setPollError\(/);
});

test('the next successful poll clears the poll error', () => {
  const load = source.slice(source.indexOf('const load = useCallback('), source.indexOf('useEffect(() => {'));
  assert.ok(load.length > 0);
  const applied = load.indexOf('setHost(next.host)');
  assert.ok(applied > 0);
  assert.match(load.slice(applied), /setPollError\(''\)/);
});

test('the setup and remove dialogs show only the action error', () => {
  const setupDialog = source.slice(source.indexOf('{open && <ModalShell'), source.indexOf('{confirmRemove && <ModalShell'));
  assert.ok(setupDialog.length > 0);
  assert.doesNotMatch(setupDialog, /pollError/);
  assert.match(source, /const shownError = error \|\| pollError/);
});
