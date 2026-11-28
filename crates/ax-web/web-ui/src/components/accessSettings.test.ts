import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { issuedTokenAfter } from '../lib/accessToken.ts';

const source = readFileSync(new URL('./AccessSettingsSection.tsx', import.meta.url), 'utf8');

test('a shown token survives later mutations that issue no new token', () => {
  const shown = { value: 'axc_secret', clientName: 'Demo' };
  assert.deepEqual(issuedTokenAfter(shown, { updated: true }, undefined), shown);
  assert.deepEqual(issuedTokenAfter(shown, {}, 'Other'), shown);
});

test('a newly issued token replaces the one on screen', () => {
  const shown = { value: 'axc_old', clientName: 'Demo' };
  assert.deepEqual(issuedTokenAfter(shown, { credential: { token: 'axc_new' } }, 'Demo'), { value: 'axc_new', clientName: 'Demo' });
  assert.deepEqual(issuedTokenAfter(null, { credential: { token: 'axc_first' } }, 'Demo'), { value: 'axc_first', clientName: 'Demo' });
});

test('a mutation does not clear the token banner', () => {
  const mutate = source.slice(source.indexOf('async function mutate('), source.indexOf('async function addMember('));
  assert.ok(mutate.length > 0);
  assert.doesNotMatch(mutate, /setIssuedToken\(null\)/);
  assert.match(mutate, /issuedTokenAfter\(/);
});

test('the password dialog shows the server error', () => {
  const modal = source.slice(source.indexOf('{passwordFor && <ModalShell'), source.indexOf('{form && <ModalShell'));
  assert.ok(modal.length > 0);
  assert.match(modal, /\{error && <p role="alert"[^>]*>\{error\}<\/p>\}/);
});
