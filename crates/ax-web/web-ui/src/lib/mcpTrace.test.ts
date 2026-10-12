import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseTraceEntry, extractPayloadJson } from './mcpTrace.ts';
test('replicated trace retains its origin and tool payload', () => {
  const row=parseTraceEntry('2026-10-12T11:00:00.000Z [node:a_1] [ax-mcp] inbound tool=ax_node args={"id":"same"}');
  assert.equal(row.origin,'a_1');assert.equal(row.kind,'inbound');assert.equal(row.tool,'ax_node');assert.deepEqual(extractPayloadJson(row),{id:'same'});
  assert.equal(parseTraceEntry('2026-10-12T11:00:00.000Z [ax-mcp] inbound tool=ax_node args={}').origin,undefined);
});
