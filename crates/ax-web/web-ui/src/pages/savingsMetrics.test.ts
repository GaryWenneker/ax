import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import type { DailySavingsRow, SavingsSummary, ToolSavingsRow } from '../api.ts';
import {
  cliShare,
  computeInsights,
  dailyReductionPct,
  fmtTs,
  splitTools,
  toolCategory,
} from './savingsMetrics.ts';

function summary(over: Partial<SavingsSummary> = {}): SavingsSummary {
  return {
    period: 'week',
    from: '2026-01-01',
    to: '2026-01-07',
    mcp_calls: 0,
    graph_calls: 0,
    failed_calls: 0,
    tokens_saved_est: 0,
    net_tokens_saved_est: 0,
    counterfactual_files: 0,
    counterfactual_tokens_est: 0,
    graph_response_tokens_est: 0,
    response_tokens_est: 0,
    counterfactual_exact_files: 0,
    cost_saved_usd_est: 0,
    graph_response_cost_usd_est: 0,
    counterfactual_cost_usd_est: 0,
    policy_calls: 0,
    success_rate_pct: 100,
    avg_duration_ms: 0,
    projects_active: 0,
    clamp_tokens_absorbed: 0,
    graph_calls_with_savings: 0,
    cli_calls: 0,
    cli_tokens_saved_est: 0,
    pricing: {} as SavingsSummary['pricing'],
    assumptions: {} as SavingsSummary['assumptions'],
    by_tool: [],
    by_project: [],
    by_model: [],
    by_weekday: [],
    by_hour: [],
    timeline: [],
    daily: [],
    recent_calls: [],
    agent_sessions: [],
    db_path: '',
    ...over,
  };
}

function tool(name: string, over: Partial<ToolSavingsRow> = {}): ToolSavingsRow {
  return {
    tool: name,
    calls: 1,
    graph_calls: 0,
    failed_calls: 0,
    tokens_saved_est: 0,
    counterfactual_files: 0,
    counterfactual_tokens_est: 0,
    graph_response_tokens_est: 0,
    avg_duration_ms: 0,
    ...over,
  };
}

function day(over: Partial<DailySavingsRow> = {}): DailySavingsRow {
  return {
    date: '2026-01-01',
    tokens_saved_est: 0,
    calls: 0,
    graph_calls: 0,
    failed_calls: 0,
    counterfactual_files: 0,
    counterfactual_tokens_est: 0,
    graph_response_tokens_est: 0,
    cost_saved_usd_est: 0,
    ...over,
  };
}

const ids = (s: SavingsSummary) => computeInsights(s).map((i) => i.id);

describe('CLI share', () => {
  it('reads CLI calls and tokens, and treats a missing share as zero', () => {
    assert.deepEqual(cliShare({ cli_calls: 2, cli_tokens_saved_est: 40 }), {
      calls: 2,
      tokens: 40,
    });
    assert.deepEqual(cliShare({}), { calls: 0, tokens: 0 });
  });
});

describe('tool category', () => {
  it('falls back to the known name lists when no row data says otherwise', () => {
    assert.equal(toolCategory('ax_explore'), 'graph');
    assert.equal(toolCategory('ax_preflight'), 'policy');
    assert.equal(toolCategory('ax_path'), 'other');
    assert.equal(toolCategory('ax_path', 0), 'other');
  });

  it('treats any tool the server counted graph calls for as a graph tool', () => {
    assert.equal(toolCategory('ax_path', 3), 'graph');
    assert.equal(toolCategory('ax_neural_search', 1), 'graph');
  });

  it('splits rows by the same rule', () => {
    const groups = splitTools([
      tool('ax_explore'),
      tool('ax_insights', { graph_calls: 2, tokens_saved_est: 50 }),
      tool('ax_preflight'),
      tool('ax_remember'),
    ]);
    assert.deepEqual(groups.graph.map((t) => t.tool), ['ax_explore', 'ax_insights']);
    assert.deepEqual(groups.policy.map((t) => t.tool), ['ax_preflight']);
    assert.deepEqual(groups.other.map((t) => t.tool), ['ax_remember']);
  });

  it('splits an empty list into three empty groups', () => {
    assert.deepEqual(splitTools([]), { graph: [], policy: [], other: [] });
  });
});

describe('daily reduction', () => {
  it('is zero when there is no counterfactual', () => {
    assert.equal(dailyReductionPct(day()), 0);
    assert.equal(dailyReductionPct(day({ counterfactual_tokens_est: -5, graph_response_tokens_est: 3 })), 0);
  });

  it('rounds the share of tokens avoided', () => {
    assert.equal(dailyReductionPct(day({ counterfactual_tokens_est: 1000, graph_response_tokens_est: 250 })), 75);
    assert.equal(dailyReductionPct(day({ counterfactual_tokens_est: 3, graph_response_tokens_est: 2 })), 33);
  });

  it('never returns NaN or Infinity', () => {
    for (const cf of [0, -1, 1, 10]) {
      assert.ok(Number.isFinite(dailyReductionPct(day({ counterfactual_tokens_est: cf, graph_response_tokens_est: 5 }))));
    }
  });
});

describe('insights', () => {
  it('returns nothing for an empty summary', () => {
    assert.deepEqual(computeInsights(summary()), []);
  });

  it('skips the hit rate when there were no graph calls', () => {
    assert.ok(!ids(summary({ graph_calls: 0, graph_calls_with_savings: 0 })).includes('savings-hit-rate'));
  });

  it('reports the share of graph calls that saved tokens', () => {
    const hit = computeInsights(summary({ graph_calls: 8, graph_calls_with_savings: 6 })).find(
      (i) => i.id === 'savings-hit-rate',
    );
    assert.equal(hit?.value, '75%');
  });

  it('reports failures with the overall success rate', () => {
    const f = computeInsights(summary({ failed_calls: 2, success_rate_pct: 90 })).find((i) => i.id === 'failures');
    assert.ok(f);
    assert.match(f.detail, /90% success rate/);
  });

  it('reports a non-zero clamp in either direction and skips zero', () => {
    assert.ok(ids(summary({ clamp_tokens_absorbed: 12 })).includes('clamp'));
    assert.ok(ids(summary({ clamp_tokens_absorbed: -12 })).includes('clamp'));
    assert.ok(!ids(summary({ clamp_tokens_absorbed: 0 })).includes('clamp'));
  });

  it('picks the best day and top tool by tokens saved and ignores all-zero rows', () => {
    const s = summary({
      daily: [day({ date: 'a', tokens_saved_est: 5 }), day({ date: 'b', tokens_saved_est: 9 })],
      by_tool: [tool('ax_node', { tokens_saved_est: 1 }), tool('ax_explore', { tokens_saved_est: 7 })],
    });
    const out = computeInsights(s);
    assert.equal(out.find((i) => i.id === 'best-day')?.value, 'b');
    assert.equal(out.find((i) => i.id === 'top-tool')?.value, 'ax_explore');
    assert.deepEqual(ids(summary({ daily: [day()], by_tool: [tool('ax_node')] })), []);
  });

  it('does not reorder the caller arrays', () => {
    const daily = [day({ date: 'a', tokens_saved_est: 1 }), day({ date: 'b', tokens_saved_est: 9 })];
    computeInsights(summary({ daily }));
    assert.deepEqual(daily.map((d) => d.date), ['a', 'b']);
  });
});

describe('timestamp format', () => {
  it('formats a millisecond timestamp to a short local date and time', () => {
    const out = fmtTs(Date.UTC(2026, 0, 15, 12, 0));
    assert.equal(typeof out, 'string');
    assert.match(out, /1[456]/);
  });
});
