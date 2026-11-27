import assert from 'node:assert/strict';
import { test } from 'node:test';
import { classifyTraceBody, traceKindBadge } from './mcpTrace.ts';

test('filter chips show the same short badge the rows use, hooks and agents included', () => {
  assert.equal(traceKindBadge('hook'), 'HOOK');
  assert.equal(traceKindBadge('agent'), 'AGT');
  assert.equal(traceKindBadge('other'), 'LOG');
  for (const body of ['[ax] hook pre-tool', '[ax] agent spawn', '[ax] cli cmd=status', 'inbound tool=ax_node', 'outbound preview x', '[ax] plugin load', 'policy index']) {
    const { kind, badge } = classifyTraceBody(body);
    assert.equal(traceKindBadge(kind), badge, body);
  }
});
