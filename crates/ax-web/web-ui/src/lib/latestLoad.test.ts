import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createLatestGate, dataAfterFailedLoad } from './latestLoad.ts';

test('a failed background refresh keeps what is on screen; a failed user load clears it', () => {
  const shown = { total: 3 };
  assert.equal(dataAfterFailedLoad(shown, true), shown);
  assert.equal(dataAfterFailedLoad(shown, false), null);
});

test('only the newest request may apply its response', () => {
  const gate = createLatestGate();
  const first = gate.begin();
  const second = gate.begin();
  assert.equal(gate.isLatest(first), false);
  assert.equal(gate.isLatest(second), true);
});
