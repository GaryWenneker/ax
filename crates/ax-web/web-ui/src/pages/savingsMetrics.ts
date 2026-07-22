import type { DailySavingsRow, SavingsSummary, ToolSavingsRow } from '../api';

export interface SavingsInsight {
  id: string;
  label: string;
  value: string;
  detail: string;
}

export function computeInsights(data: SavingsSummary): SavingsInsight[] {
  const insights: SavingsInsight[] = [];

  if (data.daily.length > 0) {
    const bestDay = [...data.daily].sort((a, b) => b.tokens_saved_est - a.tokens_saved_est)[0];
    if (bestDay && bestDay.tokens_saved_est > 0) {
      insights.push({
        id: 'best-day',
        label: 'Best day',
        value: bestDay.date,
        detail: `${bestDay.tokens_saved_est.toLocaleString()} tokens saved`,
      });
    }
  }

  const topTool = [...data.by_tool].sort((a, b) => b.tokens_saved_est - a.tokens_saved_est)[0];
  if (topTool && topTool.tokens_saved_est > 0) {
    insights.push({
      id: 'top-tool',
      label: 'Top tool',
      value: topTool.tool,
      detail: `${topTool.tokens_saved_est.toLocaleString()} tokens saved`,
    });
  }

  const topProject = data.by_project[0];
  if (topProject && topProject.tokens_saved_est > 0) {
    insights.push({
      id: 'top-project',
      label: 'Top project',
      value: topProject.project,
      detail: `${topProject.tokens_saved_est.toLocaleString()} tokens · ${topProject.graph_calls} graph calls`,
    });
  }

  const topModel = data.by_model[0];
  if (topModel && (topModel.tokens_saved_est > 0 || topModel.session_input_tokens > 0)) {
    insights.push({
      id: 'top-model',
      label: 'Top model',
      value: topModel.model,
      detail:
        topModel.tokens_saved_est > 0
          ? `${topModel.tokens_saved_est.toLocaleString()} tokens saved · ${topModel.sessions} session(s)`
          : `${topModel.session_input_tokens.toLocaleString()} input tokens · ${topModel.sessions} session(s)`,
    });
  }

  if (data.clamp_tokens_absorbed !== 0) {
    insights.push({
      id: 'clamp',
      label: 'Clamp absorbed',
      value: data.clamp_tokens_absorbed.toLocaleString(),
      detail: 'Per-call clamp vs aggregate net difference',
    });
  }

  if (data.graph_calls > 0) {
    const hitRate = Math.round((data.graph_calls_with_savings / data.graph_calls) * 100);
    insights.push({
      id: 'savings-hit-rate',
      label: 'Calls with savings',
      value: `${hitRate}%`,
      detail: `${data.graph_calls_with_savings.toLocaleString()} of ${data.graph_calls.toLocaleString()} graph calls saved tokens`,
    });
  }

  if (data.by_weekday.length > 0) {
    const topWd = [...data.by_weekday].sort((a, b) => b.tokens_saved_est - a.tokens_saved_est)[0];
    if (topWd && topWd.tokens_saved_est > 0) {
      insights.push({
        id: 'top-weekday',
        label: 'Busiest weekday',
        value: topWd.label,
        detail: `${topWd.tokens_saved_est.toLocaleString()} tokens · ${topWd.graph_calls} graph calls`,
      });
    }
  }

  if (data.failed_calls > 0) {
    insights.push({
      id: 'failures',
      label: 'Failed calls',
      value: data.failed_calls.toLocaleString(),
      detail: `${data.success_rate_pct}% success rate overall`,
    });
  }

  return insights;
}

export function dailyReductionPct(d: DailySavingsRow): number {
  if (d.counterfactual_tokens_est <= 0) return 0;
  return Math.round(
    ((d.counterfactual_tokens_est - d.graph_response_tokens_est) / d.counterfactual_tokens_est) * 100,
  );
}

export function toolCategory(tool: string): 'graph' | 'policy' | 'other' {
  const graph = [
    'ax_explore', 'ax_context', 'ax_node', 'ax_search', 'ax_callers', 'ax_callees', 'ax_impact', 'ax_affected',
  ];
  const policy = [
    'ax_preflight', 'ax_rules', 'ax_skill', 'ax_guard', 'ax_policy_capture', 'ax_status', 'ax_index',
  ];
  if (graph.includes(tool)) return 'graph';
  if (policy.includes(tool)) return 'policy';
  return 'other';
}

export function fmtTs(ms: number): string {
  const d = new Date(ms);
  return d.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function splitTools(tools: ToolSavingsRow[]) {
  const graph: ToolSavingsRow[] = [];
  const policy: ToolSavingsRow[] = [];
  const other: ToolSavingsRow[] = [];
  for (const t of tools) {
    const c = toolCategory(t.tool);
    if (c === 'graph') graph.push(t);
    else if (c === 'policy') policy.push(t);
    else other.push(t);
  }
  return { graph, policy, other };
}
