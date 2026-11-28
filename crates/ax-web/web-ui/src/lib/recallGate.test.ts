import assert from 'node:assert/strict';
import { test } from 'node:test';
import { recallStillApplies } from './recallGate.ts';

const sent = { q: 'auth', archive: false, session: '' };

test('a recall answer applies while the search box, scope and session are unchanged', () => {
  assert.equal(recallStillApplies(sent, { q: 'auth', archive: false, session: '' }), true);
  assert.equal(recallStillApplies(sent, { q: ' auth ', archive: false, session: '' }), true);
});

test('a recall answer is dropped once the search box was cleared or changed', () => {
  assert.equal(recallStillApplies(sent, { q: '', archive: false, session: '' }), false);
  assert.equal(recallStillApplies(sent, { q: 'auth token', archive: false, session: '' }), false);
});

test('a hot recall answer is dropped after Archive was switched on, and the other way round', () => {
  assert.equal(recallStillApplies(sent, { q: 'auth', archive: true, session: '' }), false);
  assert.equal(recallStillApplies({ ...sent, archive: true }, { q: 'auth', archive: false, session: '' }), false);
});

test('a recall answer is dropped once a session filter is set', () => {
  assert.equal(recallStillApplies(sent, { q: 'auth', archive: false, session: 'axs_1' }), false);
});
