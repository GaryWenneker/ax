import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { replicationErrorSlot } from './replicationErrors.ts';

const closed = { setup: false, restart: false, remove: false, stop: false };

test('the error shows on the page only while no replication dialog is open', () => {
  assert.equal(replicationErrorSlot(closed), 'page');
});

test('the error shows inside whichever replication dialog is open', () => {
  assert.equal(replicationErrorSlot({ ...closed, setup: true }), 'setup');
  assert.equal(replicationErrorSlot({ ...closed, restart: true }), 'restart');
  assert.equal(replicationErrorSlot({ ...closed, remove: true }), 'remove');
  assert.equal(replicationErrorSlot({ ...closed, stop: true }), 'stop');
});

test('every replication dialog renders the error in its own slot', () => {
  const source = readFileSync(new URL('../components/ReplicationSetup.tsx', import.meta.url), 'utf8');
  for (const slot of ['page', 'setup', 'restart', 'remove', 'stop']) {
    assert.match(source, new RegExp(`errorIn\\('${slot}'\\)`), `missing error slot ${slot}`);
  }
  assert.doesNotMatch(source, /error && !setupOpen/);
});
