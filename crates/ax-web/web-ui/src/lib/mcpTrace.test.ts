import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseTraceEntry, extractPayloadJson, shouldPulseContextRank, isContextRankEntry, nextRankMotion, rankMotionActive, rankStarActive, RANK_TURN_QUIET_MS } from './mcpTrace.ts';
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

test('a live preflight pulses the Neural legend; history and other tools do not', () => {
  const preflight = parseTraceEntry(
    '2026-11-18T12:00:00.000Z [ax] inbound tool=ax_preflight args={"prompt":"rank"}',
  );
  const returned = parseTraceEntry(
    '2026-11-18T12:00:01.000Z [ax-mcp] outbound tool=ax_preflight mode=lean text_chars=4',
  );
  const explore = parseTraceEntry(
    '2026-11-18T12:00:02.000Z [ax] inbound tool=ax_explore args={"query":"neural"}',
  );
  const status = parseTraceEntry(
    '2026-11-18T12:00:03.000Z [ax] inbound tool=ax_neural_status args={}',
  );

  assert.equal(shouldPulseContextRank([preflight], true), true);
  assert.equal(shouldPulseContextRank([returned], true), true);
  assert.equal(shouldPulseContextRank([preflight], false), false);
  assert.equal(shouldPulseContextRank([explore, status], true), false);
  assert.equal(shouldPulseContextRank([], true), false);
});

test('only a preflight row carries the context-ranking star', () => {
  assert.equal(isContextRankEntry({ tool: 'ax_preflight' }), true);
  assert.equal(isContextRankEntry({ tool: 'ax_explore' }), false);
  assert.equal(isContextRankEntry({ tool: 'ax_neural_status' }), false);
  assert.equal(isContextRankEntry({ tool: null }), false);
});

test('only the latest preflight sparkle animates while that turn is still receiving lines', () => {
  const line = (turnId: string | null, tool: string | null, id: string) => ({ turnId, tool, id });
  const idle = { turnId: null, entryId: null, untilMs: 0 };
  const t0 = 1_000_000;

  const started = nextRankMotion(idle, [line('turn-a', 'ax_preflight', 'p1')], true, t0);
  assert.deepEqual(started, { turnId: 'turn-a', entryId: 'p1', untilMs: t0 + RANK_TURN_QUIET_MS });
  assert.equal(rankMotionActive(started, t0 + 1), true);
  assert.equal(rankStarActive(started, 'p1', t0 + 1), true);
  assert.equal(rankStarActive(started, 'p0', t0 + 1), false);
  assert.equal(rankMotionActive(started, started.untilMs), false);

  const newer = nextRankMotion(started, [line('turn-a', 'ax_preflight', 'p2')], true, t0 + 1_000);
  assert.equal(newer.entryId, 'p2');
  assert.equal(rankStarActive(newer, 'p2', t0 + 1_001), true);
  assert.equal(rankStarActive(newer, 'p1', t0 + 1_001), false);

  const batch = nextRankMotion(
    idle,
    [line('turn-a', 'ax_preflight', 'p1'), line('turn-a', 'ax_preflight', 'p3')],
    true,
    t0,
  );
  assert.equal(batch.entryId, 'p3');
  assert.equal(rankStarActive(batch, 'p1', t0 + 1), false);

  const kept = nextRankMotion(newer, [line('turn-a', 'ax_explore', 'e1')], true, t0 + 5_000);
  assert.equal(kept.turnId, 'turn-a');
  assert.equal(kept.entryId, 'p2');
  assert.equal(kept.untilMs, t0 + 5_000 + RANK_TURN_QUIET_MS);

  assert.deepEqual(nextRankMotion(idle, [line('turn-a', 'ax_preflight', 'p1')], false, t0), idle);
  assert.deepEqual(nextRankMotion(idle, [line('turn-a', 'ax_explore', 'e1')], true, t0), idle);
  assert.deepEqual(nextRankMotion(idle, [line(null, 'ax_preflight', 'p1')], true, t0), idle);
  assert.deepEqual(nextRankMotion(started, [], true, t0), started);

  const blank = nextRankMotion(started, [line('', 'ax_explore', 'e2')], true, t0 + 1);
  assert.equal(blank.turnId, 'turn-a');
  assert.equal(blank.entryId, 'p1');
  assert.equal(blank.untilMs, started.untilMs);

  const ended = nextRankMotion(kept, [line('turn-b', 'ax_explore', 'e3')], true, t0 + 7_000);
  assert.deepEqual(ended, { turnId: null, entryId: null, untilMs: t0 + 7_000 });
  assert.equal(rankMotionActive(ended, t0 + 7_000), false);
  assert.equal(rankStarActive(ended, 'p2', t0 + 7_000), false);

  const resumed = nextRankMotion(started, [line('  turn-a  ', 'ax_node', 'n1')], true, started.untilMs + 10);
  assert.equal(resumed.turnId, 'turn-a');
  assert.equal(resumed.entryId, 'p1');
  assert.equal(resumed.untilMs, started.untilMs + 10 + RANK_TURN_QUIET_MS);

  const parsed = parseTraceEntry(
    '2026-11-18T12:00:00.000Z [ax] inbound tool=ax_preflight turn=turn-a session=sess-1 args={}',
  );
  const fromLog = nextRankMotion(idle, [parsed], true, t0);
  assert.equal(fromLog.turnId, 'turn-a');
  assert.equal(fromLog.entryId, parsed.id);
});
