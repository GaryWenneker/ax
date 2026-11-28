import assert from 'node:assert/strict';
import { test } from 'node:test';

import { classifyTraceBody, parseTraceEntry, type TraceKind } from './mcpTrace.ts';

const ts = '2026-11-20T10:00:00.000Z';

// Each line's free text mentions another kind's keyword; the leading token decides.
const lines: [string, TraceKind][] = [
  [`${ts} [ax-mcp] 🪓 error tool=ax_preflight message=failed to enrich context`, 'error'],
  [`${ts} [ax-mcp] 🪓 internal tool=ax_explore keys=[enrich ,memory] inject_chars=1 text_chars=2`, 'internal'],
  [`${ts} [ax-mcp] 🪓 outbound tool=ax_explore mode=lean text_chars=12 duration_ms=40`, 'outbound'],
  [`${ts} [ax-mcp] 🪓 outbound preview tool=ax_explore text=the enrich step and the memory plugin`, 'preview'],
  [`${ts} [ax-mcp] 🪓 inbound tool=ax_search args={"q":"enrich memory plugin lsp"}`, 'inbound'],
  [`${ts} [ax-mcp] 🪓 enrich policy matched_rules=1 matched_skills=0 inject_chars=10 mode=lean`, 'enrich'],
  [`${ts} [ax] 🪓 inbound tool=ax_explore args={"query":"share the policy"}`, 'inbound'],
  [`${ts} [ax] 🪓 error tool=memory message=failed to enrich memory policy duration_ms=3`, 'error'],
  [`${ts} [ax] cli cmd=recall ok q=share memory policy`, 'cli'],
  [`${ts} [ax] 🪓 cli activity op=turn.completed status=success source=cli summary=fixed the memory plugin`, 'cli'],
  [`${ts} [ax] 🪓 hook activity op=pre-tool status=success source=hook summary=checked lsp policy`, 'hook'],
  [`${ts} [ax] 🪓 agent activity op=spawn status=success source=agent summary=share workspace`, 'agent'],
  [`${ts} [ax] 🪓 mcp-activity activity op=tool status=success source=mcp summary=memory policy`, 'inbound'],
  [`${ts} [ax] 🪓 memory remember ok id=m1 kind=note summary=policy share tool=memory`, 'memory'],
  [`${ts} [ax] 🪓 policy index ok rules=1 skills=2 tool=policy`, 'policy'],
  [`${ts} [ax] 🪓 plugin extract name=echo ok tool=plugin`, 'plugin'],
  [`${ts} [ax] 🪓 lsp enrich start limit=20 via=mcp tool=lsp`, 'lsp'],
  [`${ts} [ax] 🪓 ship ok mode=evaluate passed=1 tool=ship`, 'ship'],
  [`${ts} [ax] 🪓 ship-ci ok mode=ci passed=1 tool=ship-ci`, 'ship'],
  [`${ts} [ax] 🪓 share push ok tool=share`, 'share'],
  [`${ts} [ax] 🪓 workspace sync ok files=19 duration_ms=162 tool=workspace`, 'workspace'],
  [`${ts} [ax] 🪓 embed batch ok count=3 tool=embed`, 'embed'],
  [`${ts} [ax] 🪓 action lsp start tool=action`, 'action'],
  [`${ts} [ax] 🪓 token budget: narrow query about memory policy`, 'other'],
];

for (const [raw, kind] of lines) {
  test(`classifies by leading token: ${raw.slice(ts.length + 1)}`, () => {
    assert.equal(parseTraceEntry(raw).kind, kind);
  });
}

test('bodies without a logger prefix classify by their leading token too', () => {
  assert.equal(classifyTraceBody('policy index').kind, 'policy');
  assert.equal(classifyTraceBody('outbound preview x').kind, 'preview');
  assert.equal(classifyTraceBody('[ax] cli cmd=explore ok').kind, 'cli');
  assert.equal(classifyTraceBody('[ax] hook pre-tool').kind, 'hook');
});
