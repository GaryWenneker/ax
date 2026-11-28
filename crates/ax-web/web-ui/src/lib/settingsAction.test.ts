import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { runSettingsAction } from './settingsAction.ts';

test('a rejected action reports failure and skips the refresh', async () => {
  let reloaded = false;
  const result = await runSettingsAction(
    async () => { throw new Error('Configure a password or a passkey first'); },
    async () => { reloaded = true; },
  );
  assert.deepEqual(result, { ok: false, error: 'Configure a password or a passkey first' });
  assert.equal(reloaded, false);
});

test('a refresh failure after a successful action still reports success', async () => {
  const result = await runSettingsAction(
    async () => undefined,
    async () => { throw new Error('offline'); },
  );
  assert.equal(result.ok, true);
  assert.match(result.error ?? '', /could not refresh.*offline/i);
});

test('a successful action and refresh report no error', async () => {
  assert.deepEqual(await runSettingsAction(async () => 1, async () => undefined), { ok: true, error: null });
});

const source = readFileSync(new URL('../components/SecuritySettingsSection.tsx', import.meta.url), 'utf8');
function body(start: string, end: string) {
  const from = source.indexOf(start);
  const to = source.indexOf(end, from + 1);
  assert.ok(from >= 0 && to > from, `missing ${start}`);
  return source.slice(from, to);
}

test('auth-changed events and the password reset wait for a successful request', () => {
  assert.match(body('async function setup(', 'async function toggle('), /if \(await execute\([\s\S]*\)\) setPassword\(''\)/);
  assert.match(body('async function toggle(', 'async function generateCodes('), /if \(await execute\([\s\S]*\)\) window\.dispatchEvent/);
  assert.match(body('async function revokeSession(', 'async function signout('), /if \(!await execute\([\s\S]*\)\) return;/);
  assert.match(body('async function signout(', 'return <div'), /if \(await execute\([\s\S]*\)\) window\.dispatchEvent/);
});

test('copying recovery codes reports whether the clipboard accepted them', () => {
  const copy = body('async function copyCodes(', 'return <div');
  assert.match(copy, /setNotice\(/);
  assert.match(copy, /setError\(/);
  assert.doesNotMatch(source, /void navigator\.clipboard\.writeText\(recoveryCodes/);
});
