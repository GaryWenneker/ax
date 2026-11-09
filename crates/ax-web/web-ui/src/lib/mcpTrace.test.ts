import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseTraceEntry, extractPayloadJson } from './mcpTrace.ts';
test('replicated trace retains its origin and tool payload', () => {
  const row=parseTraceEntry('2026-10-12T11:00:00.000Z [node:a_1] [ax-mcp] inbound tool=ax_node args={"id":"same"}');
  assert.equal(row.origin,'a_1');assert.equal(row.kind,'inbound');assert.equal(row.tool,'ax_node');assert.deepEqual(extractPayloadJson(row),{id:'same'});
  assert.equal(parseTraceEntry('2026-10-12T11:00:00.000Z [ax-mcp] inbound tool=ax_node args={}').origin,undefined);
});

test('CLI-branded tool traces classify as inbound and outbound', () => {
  const inbound = parseTraceEntry(
    '2026-11-09T11:00:00.000Z [ax] 🪓 inbound tool=ax_explore args={"query":"logging"}',
  );
  assert.equal(inbound.kind, 'inbound');
  assert.equal(inbound.tool, 'ax_explore');

  const outbound = parseTraceEntry(
    '2026-11-09T11:00:01.000Z [ax] 🪓 outbound tool=ax_explore mode=lean text_chars=12 structured=false duration_ms=40',
  );
  assert.equal(outbound.kind, 'outbound');
  assert.equal(outbound.tool, 'ax_explore');

  const memory = parseTraceEntry(
    '2026-11-09T11:00:02.000Z [ax] 🪓 memory remember ok id=m1 kind=note tool=memory',
  );
  assert.equal(memory.kind, 'memory');
});
