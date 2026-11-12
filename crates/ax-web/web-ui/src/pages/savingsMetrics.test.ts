import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { cliShare, toolCategory } from './savingsMetrics.ts';

describe('CLI share', () => {
  it('reads CLI calls and tokens, and treats a missing share as zero', () => {
    assert.deepEqual(cliShare({ cli_calls: 2, cli_tokens_saved_est: 40 }), {
      calls: 2,
      tokens: 40,
    });
    assert.deepEqual(cliShare({}), { calls: 0, tokens: 0 });
  });

  it('keeps tool buckets on MCP names', () => {
    assert.equal(toolCategory('ax_explore'), 'graph');
    assert.equal(toolCategory('ax_preflight'), 'policy');
    assert.equal(toolCategory('ax_path'), 'other');
  });
});
