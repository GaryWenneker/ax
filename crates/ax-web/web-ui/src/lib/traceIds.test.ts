import assert from 'node:assert/strict';
import { test } from 'node:test';
import { traceIdsFromMessage } from './traceIds.ts';

test('session and turn fields become TraceEntry ids', () => {
  assert.deepEqual(
    traceIdsFromMessage('inbound tool=ax_explore session=conv-1 turn=gen-1'),
    { sessionId: 'conv-1', turnId: 'gen-1' },
  );
  assert.deepEqual(traceIdsFromMessage('inbound tool=ax_explore'), {
    sessionId: null,
    turnId: null,
  });
});
