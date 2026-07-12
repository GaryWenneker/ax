import { useCallback, useEffect, useState, type ReactNode } from 'react';

import { fetchSavings, importSavingsSessions, type SavingsSummary, type DailySavingsRow, type ToolSavingsRow, type ProjectSavingsRow, type RecentCallRow, type WeekdaySavingsRow } from '../api';
import { computeInsights, dailyReductionPct, fmtTs, splitTools, toolCategory, type SavingsInsight } from './savingsMetrics';
import { InfoHover } from '../components/ui/InfoHover';
import {
  BusyLabel,
  DataTable,
  DistBar,
  FilterBar,
  PageCard,
  PageCardBody,
  PageEmpty,
  PageHero,
  PageLoading,
  PageShell,
  PageToasts,
} from '../components/ui/PageLayout';
import { usePageContext } from '../context/UiContext';

type Period = 'week' | 'month_to_date' | 'month' | 'year' | 'custom';

const PERIOD_OPTIONS: Array<{ value: Period; label: string }> = [
  { value: 'week', label: 'Week' },
  { value: 'month_to_date', label: 'Month to date' },
  { value: 'month', label: 'Month' },
  { value: 'year', label: 'Year' },
  { value: 'custom', label: 'Custom range' },
];

function fmt(n: number): string {
  return n.toLocaleString();
}

function fmtCompact(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

const TOOL_TIPS: Record<string, string> = {
  ax_explore: 'Natural-language code graph search — primary savings source.',
  ax_context: 'Builds task context from the graph instead of reading whole files.',
  ax_node: 'Single-symbol detail with callers/callees — small targeted response.',
  ax_search: 'Fast FTS symbol lookup across the indexed project.',
  ax_callers: 'Who calls this symbol — graph traversal without file reads.',
  ax_callees: 'What this symbol calls — outgoing dependency slice.',
  ax_impact: 'Blast-radius subgraph for a symbol or change.',
  ax_affected: 'Tests affected by file changes via graph + TIA.',
  ax_files: 'Lists indexed files — minimal response, low savings.',
  ax_preflight: 'Policy/rules inject — not savings-eligible by design.',
  ax_rules: 'Lists indexed policy rules — not savings-eligible.',
  ax_skill: 'Loads a policy skill body — not savings-eligible.',
  ax_guard: 'Pre-write policy guard — not savings-eligible.',
  ax_policy_capture: 'Captures durable directives — not savings-eligible.',
  ax_status: 'Index stats — not savings-eligible.',
  ax_index: 'Triggers re-index — not savings-eligible.',
};

function toolTip(name: string): ReactNode {
  const full = name.startsWith('ax_') ? name : `ax_${name}`;
  return TOOL_TIPS[full] ?? 'MCP tool call logged to ~/.ax/usage.db.';
}

function fmtUsd(n: number): string {
  return n.toLocaleString(undefined, {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

function pct(part: number, whole: number): number {
  if (whole <= 0) return 0;
  return Math.round((part / whole) * 100);
}

/* ---- Hero stat card ---- */
function HeroStat({
  label,
  value,
  sub,
  tone,
  tip,
}: {
  label: string;
  value: string;
  sub?: string;
  tone?: 'ok' | 'accent' | 'warn';
  tip?: React.ReactNode;
}) {
  return (
    <div className="sv-hero-stat">
      <span className="sv-hero-stat-label">
        {label}
        {tip && <InfoHover label={`About ${label}`}>{tip}</InfoHover>}
      </span>
      <span className={`sv-hero-stat-value${tone ? ` sv-hero-stat-value--${tone}` : ''}`}>{value}</span>
      {sub && <span className="sv-hero-stat-sub">{sub}</span>}
    </div>
  );
}

/* ---- Contribution heatmap ---- */
function buildHeatmap(daily: DailySavingsRow[]): { weeks: Array<Array<{ date: string; value: number; level: number }>>; max: number } {
  if (daily.length === 0) return { weeks: [], max: 0 };

  const map = new Map(daily.map((d) => [d.date, d.tokens_saved_est]));
  const sorted = [...daily].sort((a, b) => a.date.localeCompare(b.date));
  const start = new Date(sorted[0].date);
  const end = new Date(sorted[sorted.length - 1].date);

  start.setDate(start.getDate() - start.getDay());

  const max = Math.max(...daily.map((d) => d.tokens_saved_est), 1);
  const weeks: Array<Array<{ date: string; value: number; level: number }>> = [];
  let week: Array<{ date: string; value: number; level: number }> = [];

  const cur = new Date(start);
  while (cur <= end || week.length > 0) {
    const iso = cur.toISOString().slice(0, 10);
    const val = map.get(iso) ?? 0;
    const level = val === 0 ? 0 : val <= max * 0.25 ? 1 : val <= max * 0.5 ? 2 : val <= max * 0.75 ? 3 : 4;
    week.push({ date: iso, value: val, level });
    if (week.length === 7) {
      weeks.push(week);
      week = [];
    }
    cur.setDate(cur.getDate() + 1);
    if (cur > end && week.length === 0) break;
    if (cur > new Date(end.getTime() + 7 * 86400000)) break;
  }
  if (week.length > 0) weeks.push(week);
  return { weeks, max };
}

function ContributionHeatmap({ daily }: { daily: DailySavingsRow[] }) {
  const { weeks, max } = buildHeatmap(daily);
  if (weeks.length === 0) return null;

  const sorted = [...daily].sort((a, b) => a.date.localeCompare(b.date));
  const months: Array<{ label: string; col: number }> = [];
  let lastMonth = '';
  weeks.forEach((week, i) => {
    const first = week[0];
    if (!first) return;
    const m = first.date.slice(0, 7);
    if (m !== lastMonth) {
      const d = new Date(first.date);
      months.push({ label: d.toLocaleString(undefined, { month: 'short' }), col: i });
      lastMonth = m;
    }
  });

  const activeDays = daily.filter((d) => d.tokens_saved_est > 0).length;
  const totalDays = weeks.length * 7;

  return (
    <div className="sv-heatmap-section">
      <div className="sv-heatmap-stats">
        <span className="sv-heatmap-stat">{activeDays} active day{activeDays !== 1 ? 's' : ''}</span>
        <span className="sv-heatmap-stat">{sorted[0]?.date} — {sorted[sorted.length - 1]?.date}</span>
      </div>
      <div className="sv-heatmap-wrap">
        <div className="sv-heatmap-months">
          {months.map((m) => (
            <span key={m.col} className="sv-heatmap-month" style={{ gridColumn: m.col + 1 }}>{m.label}</span>
          ))}
        </div>
        <div className="sv-heatmap-grid" style={{ gridTemplateColumns: `repeat(${weeks.length}, 1fr)` }}>
          {weeks.map((week, wi) =>
            week.map((day, di) => (
              <div
                key={day.date}
                className={`sv-heatmap-cell sv-heatmap-cell--${day.level}`}
                style={{ gridColumn: wi + 1, gridRow: di + 1 }}
                title={`${day.date}: ${fmt(day.value)} tokens saved`}
              />
            ))
          )}
        </div>
        <div className="sv-heatmap-legend">
          <span className="sv-heatmap-legend-label">Less</span>
          {[0, 1, 2, 3, 4].map((l) => (
            <div key={l} className={`sv-heatmap-cell sv-heatmap-cell--${l} sv-heatmap-cell--legend`} />
          ))}
          <span className="sv-heatmap-legend-label">More</span>
        </div>
      </div>
    </div>
  );
}

/* ---- Sparkline area chart ---- */
function AreaChart({
  data,
  height = 120,
  color = 'var(--accent)',
}: {
  data: Array<{ label: string; value: number }>;
  height?: number;
  color?: string;
}) {
  if (data.length === 0) return null;
  const max = Math.max(...data.map((d) => d.value), 1);
  const w = 100;
  const h = height;
  const padY = 4;
  const usable = h - padY * 2;
  const step = data.length > 1 ? w / (data.length - 1) : w;

  const points = data.map((d, i) => ({
    x: i * step,
    y: padY + usable - (d.value / max) * usable,
  }));

  const linePath = points.map((p, i) => `${i === 0 ? 'M' : 'L'}${p.x},${p.y}`).join(' ');
  const areaPath = `${linePath} L${points[points.length - 1].x},${h} L${points[0].x},${h} Z`;

  return (
    <div className="sv-area-chart-wrap">
      <svg viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" className="sv-area-chart" style={{ height }}>
        <defs>
          <linearGradient id="areaGrad" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor={color} stopOpacity="0.4" />
            <stop offset="100%" stopColor={color} stopOpacity="0.02" />
          </linearGradient>
        </defs>
        <path d={areaPath} fill="url(#areaGrad)" />
        <path d={linePath} fill="none" stroke={color} strokeWidth="0.8" vectorEffect="non-scaling-stroke" />
        {points.map((p, i) => (
          <circle key={i} cx={p.x} cy={p.y} r="1.2" fill={color} opacity="0.7">
            <title>{data[i].label}: {fmt(data[i].value)}</title>
          </circle>
        ))}
      </svg>
      <div className="sv-area-chart-labels">
        <span>{data[0].label}</span>
        <span>{data[data.length - 1].label}</span>
      </div>
    </div>
  );
}

function InsightsStrip({ insights }: { insights: SavingsInsight[] }) {
  if (insights.length === 0) return null;
  return (
    <div className="sv-insights">
      {insights.map((i) => (
        <div key={i.id} className="sv-insight">
          <span className="sv-insight-label">{i.label}</span>
          <span className="sv-insight-value">{i.value}</span>
          <span className="sv-insight-detail">{i.detail}</span>
        </div>
      ))}
    </div>
  );
}

function ToolTable({ tools, totalSaved }: { tools: ToolSavingsRow[]; totalSaved: number }) {
  const sorted = [...tools].sort((a, b) => b.tokens_saved_est - a.tokens_saved_est);
  return (
    <DataTable>
      <thead>
        <tr>
          <th>Tool</th>
          <th>Type</th>
          <th>Calls</th>
          <th>Graph</th>
          <th>Failed</th>
          <th>Without ax</th>
          <th>With ax</th>
          <th>Saved</th>
          <th>Files</th>
          <th>Avg ms</th>
          <th>Share</th>
        </tr>
      </thead>
      <tbody>
        {sorted.map((t) => {
          const share = totalSaved > 0 ? Math.round((t.tokens_saved_est / totalSaved) * 100) : 0;
          const cat = toolCategory(t.tool);
          return (
            <tr key={t.tool} className={cat === 'policy' ? 'sv-row-dim' : undefined}>
              <td>
                <span className="sv-tool-name">
                  {t.tool}
                  <InfoHover label={`About ${t.tool}`}>{toolTip(t.tool)}</InfoHover>
                </span>
              </td>
              <td><span className={`sv-tool-cat sv-tool-cat--${cat}`}>{cat}</span></td>
              <td className="num">{fmt(t.calls)}</td>
              <td className="num">{fmt(t.graph_calls)}</td>
              <td className="num">{t.failed_calls > 0 ? fmt(t.failed_calls) : '—'}</td>
              <td className="num">{t.counterfactual_tokens_est > 0 ? fmtCompact(t.counterfactual_tokens_est) : '—'}</td>
              <td className="num">{t.graph_response_tokens_est > 0 ? fmtCompact(t.graph_response_tokens_est) : '—'}</td>
              <td className="num">{t.tokens_saved_est > 0 ? fmtCompact(t.tokens_saved_est) : '—'}</td>
              <td className="num">{t.counterfactual_files > 0 ? fmt(t.counterfactual_files) : '—'}</td>
              <td className="num">{t.avg_duration_ms > 0 ? fmt(t.avg_duration_ms) : '—'}</td>
              <td><DistBar pct={share} /></td>
            </tr>
          );
        })}
      </tbody>
    </DataTable>
  );
}

function ProjectTable({ rows }: { rows: ProjectSavingsRow[] }) {
  if (rows.length === 0) return null;
  return (
    <DataTable>
      <thead>
        <tr>
          <th>Project</th>
          <th>Calls</th>
          <th>Graph</th>
          <th>Tokens saved</th>
          <th>Files</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((p) => (
          <tr key={p.project}>
            <td className="mono">{p.project}</td>
            <td className="num">{fmt(p.calls)}</td>
            <td className="num">{fmt(p.graph_calls)}</td>
            <td className="num">{fmtCompact(p.tokens_saved_est)}</td>
            <td className="num">{fmt(p.counterfactual_files)}</td>
          </tr>
        ))}
      </tbody>
    </DataTable>
  );
}

function RecentCallsTable({ rows }: { rows: RecentCallRow[] }) {
  if (rows.length === 0) return null;
  return (
    <DataTable>
      <thead>
        <tr>
          <th>When</th>
          <th>Tool</th>
          <th>Project</th>
          <th>Without ax</th>
          <th>With ax</th>
          <th>Saved</th>
          <th>Files</th>
          <th>ms</th>
          <th>Status</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r, i) => (
          <tr key={`${r.created_at}-${r.tool}-${i}`} className={!r.ok ? 'sv-row-warn' : undefined}>
            <td className="mono">{fmtTs(r.created_at)}</td>
            <td className="mono">{r.tool}</td>
            <td className="mono">{r.project ?? '—'}</td>
            <td className="num">{r.savings_eligible ? fmtCompact(r.counterfactual_tokens_est) : '—'}</td>
            <td className="num">{fmtCompact(r.response_tokens_est)}</td>
            <td className="num">{r.tokens_saved_est > 0 ? fmtCompact(r.tokens_saved_est) : '—'}</td>
            <td className="num">{r.counterfactual_files > 0 ? r.counterfactual_files : '—'}</td>
            <td className="num">{r.duration_ms ?? '—'}</td>
            <td>{r.ok ? (r.savings_eligible ? 'graph' : 'policy') : 'failed'}</td>
          </tr>
        ))}
      </tbody>
    </DataTable>
  );
}

/* ---- Comparison bars ---- */
function ComparisonBars({
  withoutAx,
  withAx,
  costWithout,
  costWith,
}: {
  withoutAx: number;
  withAx: number;
  costWithout: number;
  costWith: number;
}) {
  const max = Math.max(withoutAx, withAx, 1);
  const withoutPct = Math.round((withoutAx / max) * 100);
  const withPct = Math.round((withAx / max) * 100);
  const savedPct = withoutAx > 0 ? Math.round(((withoutAx - withAx) / withoutAx) * 100) : 0;
  const savedTokens = Math.max(withoutAx - withAx, 0);

  if (withoutAx === 0 && withAx === 0) {
    return (
      <PageEmpty title="No graph tool calls yet">
        Use ax_explore, ax_context, or ax_node — the comparison appears when graph MCP tools run.
      </PageEmpty>
    );
  }

  return (
    <div className="sv-comparison">
      <div className="sv-comparison-bars">
        <div className="sv-comparison-row">
          <div className="sv-comparison-label-col">
            <span className="sv-comparison-label">Without ax</span>
            <span className="sv-comparison-sublabel">Full file reads</span>
          </div>
          <div className="sv-comparison-bar-col">
            <div className="sv-comparison-bar sv-comparison-bar--without" style={{ width: `${withoutPct}%` }} />
          </div>
          <div className="sv-comparison-value-col">
            <span className="sv-comparison-value">{fmtCompact(withoutAx)}</span>
            <span className="sv-comparison-cost">{fmtUsd(costWithout)}</span>
          </div>
        </div>
        <div className="sv-comparison-row">
          <div className="sv-comparison-label-col">
            <span className="sv-comparison-label">With ax</span>
            <span className="sv-comparison-sublabel">Graph responses</span>
          </div>
          <div className="sv-comparison-bar-col">
            <div className="sv-comparison-bar sv-comparison-bar--with" style={{ width: `${withPct}%` }} />
          </div>
          <div className="sv-comparison-value-col">
            <span className="sv-comparison-value">{fmtCompact(withAx)}</span>
            <span className="sv-comparison-cost">{fmtUsd(costWith)}</span>
          </div>
        </div>
      </div>
      <div className="sv-comparison-summary">
        <span className="sv-comparison-saved">{savedPct}% reduction</span>
        <span className="sv-comparison-delta">
          {fmtCompact(savedTokens)} tokens / {fmtUsd(Math.max(costWithout - costWith, 0))} saved
        </span>
      </div>
    </div>
  );
}

function WeekdayBars({ rows }: { rows: WeekdaySavingsRow[] }) {
  if (rows.length === 0) return null;
  const ordered = [...rows].sort((a, b) => {
    const ai = a.weekday === 0 ? 7 : a.weekday;
    const bi = b.weekday === 0 ? 7 : b.weekday;
    return ai - bi;
  });
  const max = Math.max(...ordered.map((r) => r.tokens_saved_est), 1);

  return (
    <div className="sv-weekday-bars">
      {ordered.map((r) => {
        const h = Math.round((r.tokens_saved_est / max) * 100);
        return (
          <div key={r.weekday} className="sv-weekday-col" title={`${r.label}: ${fmt(r.tokens_saved_est)} tokens · ${r.graph_calls} graph calls`}>
            <div className="sv-weekday-bar-wrap">
              <div className="sv-weekday-bar" style={{ height: `${Math.max(h, r.tokens_saved_est > 0 ? 8 : 0)}%` }} />
            </div>
            <span className="sv-weekday-label">{r.label}</span>
            <span className="sv-weekday-sub">{r.graph_calls > 0 ? fmtCompact(r.tokens_saved_est) : '—'}</span>
          </div>
        );
      })}
    </div>
  );
}

/* ---- Daily comparison bars (redesigned) ---- */
function DailyBars({ daily }: { daily: DailySavingsRow[] }) {
  const eligible = daily.filter(
    (d) => d.counterfactual_tokens_est > 0 || d.graph_response_tokens_est > 0,
  );
  if (eligible.length === 0) return null;

  const max = Math.max(
    ...eligible.flatMap((d) => [d.counterfactual_tokens_est, d.graph_response_tokens_est]),
    1,
  );

  return (
    <div className="sv-daily-bars sv-daily-bars--tall">
      <div className="sv-daily-bars-legend">
        <span className="sv-daily-bars-legend-item">
          <span className="sv-daily-bars-swatch sv-daily-bars-swatch--without" />
          Without ax
        </span>
        <span className="sv-daily-bars-legend-item">
          <span className="sv-daily-bars-swatch sv-daily-bars-swatch--with" />
          With ax
        </span>
      </div>
      <div className="sv-daily-bars-grid">
        {eligible.map((d) => {
          const hWithout = Math.round((d.counterfactual_tokens_est / max) * 100);
          const hWith = Math.round((d.graph_response_tokens_est / max) * 100);
          return (
            <div key={d.date} className="sv-daily-bars-col" title={`${d.date}\nWithout: ${fmt(d.counterfactual_tokens_est)}\nWith: ${fmt(d.graph_response_tokens_est)}`}>
              <div className="sv-daily-bars-pair">
                <div className="sv-daily-bars-bar sv-daily-bars-bar--without" style={{ height: `${Math.max(hWithout, d.counterfactual_tokens_est > 0 ? 5 : 0)}%` }} />
                <div className="sv-daily-bars-bar sv-daily-bars-bar--with" style={{ height: `${Math.max(hWith, d.graph_response_tokens_est > 0 ? 5 : 0)}%` }} />
              </div>
              <span className="sv-daily-bars-date">{d.date.slice(5)}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/* ---- Efficiency metric ---- */
function EfficiencyMetric({ label, value, maxVal, detail }: { label: string; value: number; maxVal: number; detail?: string }) {
  const pctVal = maxVal > 0 ? Math.min(Math.round((value / maxVal) * 100), 100) : 0;
  return (
    <div className="sv-efficiency-row">
      <div className="sv-efficiency-header">
        <span className="sv-efficiency-label">{label}</span>
        <span className="sv-efficiency-value">{pctVal}%</span>
      </div>
      <div className="sv-efficiency-track">
        <div className="sv-efficiency-fill" style={{ width: `${pctVal}%` }} />
      </div>
      {detail && <span className="sv-efficiency-detail">{detail}</span>}
    </div>
  );
}

export default function SavingsPage() {
  const [period, setPeriod] = useState<Period>('month_to_date');
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const [data, setData] = useState<SavingsSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [importing, setImporting] = useState(false);
  const [importMsg, setImportMsg] = useState<string | null>(null);
  const [dailyView, setDailyView] = useState<'saved' | 'reduction' | 'compare' | 'weekday' | 'table'>('saved');
  const [toolFilter, setToolFilter] = useState<'all' | 'graph' | 'policy'>('all');

  const load = useCallback(async () => {
    if (period === 'custom' && !from) {
      setError('Pick a start date for custom range.');
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const summary = await fetchSavings({
        period,
        from: period === 'custom' ? from : undefined,
        to: period === 'custom' && to ? to : undefined,
      });
      setData(summary);
    } catch (e) {
      setError(String(e));
      setData(null);
    } finally {
      setLoading(false);
    }
  }, [period, from, to]);

  useEffect(() => {
    void load();
  }, [load]);

  const runImport = useCallback(async () => {
    setImporting(true);
    setImportMsg(null);
    try {
      const result = await importSavingsSessions();
      setImportMsg(
        `Imported ${result.claude_sessions} Claude + ${result.cursor_sessions} Cursor session(s)`,
      );
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setImporting(false);
    }
  }, [load]);

  usePageContext(
    'Context savings',
    data
      ? `${fmt(data.tokens_saved_est)} tok / ${fmtUsd(data.cost_saved_usd_est)} saved · ${data.from} → ${data.to}`
      : undefined,
  );

  const netPct = data ? pct(data.counterfactual_tokens_est - data.graph_response_tokens_est, data.counterfactual_tokens_est) : 0;
  const avgSavedPerCall = data && data.graph_calls > 0 ? Math.round(data.tokens_saved_est / data.graph_calls) : 0;
  const graphRatio = data && data.mcp_calls > 0 ? Math.round((data.graph_calls / data.mcp_calls) * 100) : 0;
  const exactFilePct = data && data.counterfactual_files > 0 ? Math.round((data.counterfactual_exact_files / data.counterfactual_files) * 100) : 0;

  const dailySavingsData = data?.daily.map((d) => ({
    label: d.date.slice(5),
    value: d.tokens_saved_est,
  })) ?? [];

  const totalSessionTokens = data?.agent_sessions.reduce((s, a) => s + (a.session_input_tokens ?? 0), 0) ?? 0;
  const totalSessionCost = data?.agent_sessions.reduce((s, a) => s + (a.session_cost_usd_est ?? 0), 0) ?? 0;
  const totalAxCalls = data?.agent_sessions.reduce((s, a) => s + a.ax_calls, 0) ?? 0;
  const totalReadCalls = data?.agent_sessions.reduce((s, a) => s + a.read_calls, 0) ?? 0;
  const totalGrepCalls = data?.agent_sessions.reduce((s, a) => s + a.grep_calls, 0) ?? 0;
  const axShare = (totalAxCalls + totalReadCalls + totalGrepCalls) > 0
    ? Math.round((totalAxCalls / (totalAxCalls + totalReadCalls + totalGrepCalls)) * 100)
    : 0;
  const policyPct = data && data.mcp_calls > 0 ? Math.round((data.policy_calls / data.mcp_calls) * 100) : 0;
  const insights = data ? computeInsights(data) : [];
  const toolGroups = data ? splitTools(data.by_tool) : { graph: [], policy: [], other: [] };
  const filteredTools = data
    ? toolFilter === 'graph'
      ? toolGroups.graph
      : toolFilter === 'policy'
        ? [...toolGroups.policy, ...toolGroups.other]
        : data.by_tool
    : [];

  const dailyReductionData = data?.daily.map((d) => ({
    label: d.date.slice(5),
    value: dailyReductionPct(d),
  })) ?? [];

  return (
    <PageShell className="sv-page">
      <PageHero
        title="Context savings"
        subtitle={
          data
            ? `${data.from} → ${data.to} · ${fmt(data.graph_calls)} graph calls · priced at ${data.pricing.reference_model}`
            : 'Measured token savings from ax graph MCP tools vs full file reads.'
        }
        actions={
          <div className="sv-period-bar">
            <select
              className="settings-select"
              value={period}
              onChange={(e) => setPeriod(e.target.value as Period)}
              aria-label="Time period"
            >
              {PERIOD_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>{o.label}</option>
              ))}
            </select>
            {period === 'custom' && (
              <>
                <input className="settings-input settings-input--narrow" type="date" value={from} onChange={(e) => setFrom(e.target.value)} aria-label="From date" />
                <input className="settings-input settings-input--narrow" type="date" value={to} onChange={(e) => setTo(e.target.value)} aria-label="To date" />
              </>
            )}
            <button type="button" className="btn primary" onClick={() => void load()} disabled={loading}>
              {loading ? <BusyLabel label="Loading…" /> : 'Apply'}
            </button>
            <button type="button" className="btn" onClick={() => void runImport()} disabled={importing} title="Import Cursor / Claude Code session transcripts">
              {importing ? <BusyLabel label="Importing…" /> : 'Import sessions'}
            </button>
          </div>
        }
      />

      <PageToasts err={error} />
      {importMsg && <div className="settings-toast settings-toast--ok">{importMsg}</div>}

      {loading && !data && <PageLoading label="Loading savings…" />}

      {data && (
        <div className="sv-dashboard">
          {/* ---- Hero stats strip ---- */}
          <div className="sv-hero-strip sv-hero-strip--wide">
            <HeroStat
              label="Tokens saved"
              value={fmtCompact(data.tokens_saved_est)}
              sub={`${fmt(data.tokens_saved_est)} clamped`}
              tone="ok"
              tip={<>Sum of per-call savings, each clamped at zero. Unclamped net: <strong>{fmt(data.net_tokens_saved_est)}</strong>.</>}
            />
            <HeroStat
              label="Cost saved"
              value={fmtUsd(data.cost_saved_usd_est)}
              sub={`${data.pricing.reference_model} input rate`}
              tone="ok"
              tip={<>Saved tokens priced as input at {fmtUsd(data.pricing.input_per_mtok)}/M. Config: <code>{data.pricing.config_path}</code>.</>}
            />
            <HeroStat
              label="Net reduction"
              value={`${Math.max(netPct, 0)}%`}
              sub="counterfactual → graph"
              tone="accent"
              tip={<>Without ax: <strong>{fmtCompact(data.counterfactual_tokens_est)}</strong> tokens ({fmtUsd(data.counterfactual_cost_usd_est)}). With ax: <strong>{fmtCompact(data.graph_response_tokens_est)}</strong> ({fmtUsd(data.graph_response_cost_usd_est)}).</>}
            />
            <HeroStat
              label="Graph calls"
              value={fmt(data.graph_calls)}
              sub={`${graphRatio}% of ${fmt(data.mcp_calls)} MCP`}
              tip={<>Only graph tools (explore, context, node, …) produce savings. Policy tools are logged but excluded.</>}
            />
            <HeroStat
              label="Files avoided"
              value={fmt(data.counterfactual_files)}
              sub={`${exactFilePct}% measured exactly`}
              tip={<>Referenced files tokenized with o200k BPE. <strong>{fmt(data.counterfactual_exact_files)}</strong> exact, rest estimated from line depth or avg file size.</>}
            />
            <HeroStat
              label="Avg saved / call"
              value={avgSavedPerCall > 0 ? fmtCompact(avgSavedPerCall) : '—'}
              sub="per graph call"
              tip="tokens_saved_est ÷ graph_calls for this period."
            />
            <HeroStat
              label="Success rate"
              value={`${data.success_rate_pct}%`}
              sub={`${fmt(data.mcp_calls - data.failed_calls)} / ${fmt(data.mcp_calls)} calls`}
              tone={data.success_rate_pct >= 95 ? 'ok' : data.failed_calls > 0 ? 'warn' : undefined}
              tip="Successful MCP calls ÷ all logged calls. Failed calls never contribute to savings."
            />
            <HeroStat
              label="Policy calls"
              value={fmt(data.policy_calls)}
              sub={`${policyPct}% of MCP traffic`}
              tip="Non-graph tools (preflight, rules, guard, …) — logged but not savings-eligible."
            />
            <HeroStat
              label="Projects"
              value={fmt(data.projects_active)}
              sub="distinct repos with MCP activity"
              tip="Unique project paths from mcp_call_log.project in this period."
            />
            <HeroStat
              label="Avg latency"
              value={data.avg_duration_ms > 0 ? `${data.avg_duration_ms} ms` : '—'}
              sub="MCP response time"
              tip="Mean duration_ms when the MCP server recorded wall-clock time."
            />
          </div>

          {insights.length > 0 && (
            <PageCard
              title="Highlights"
              description="Auto-derived from this period — each fact appears once here."
              info={<InfoHover label="About highlights">Computed client-side from the summary payload. Not duplicated in charts below.</InfoHover>}
            >
              <PageCardBody>
                <InsightsStrip insights={insights} />
              </PageCardBody>
            </PageCard>
          )}

          {/* ---- Activity heatmap ---- */}
          {data.daily.length > 0 && (
            <PageCard
              title="Savings activity"
              description={`${data.from} — ${data.to}`}
              info={
                <InfoHover label="About the heatmap">
                  Each cell is one day. Darker cells mean more tokens saved that day.
                  Only days with graph tool calls are colored.
                </InfoHover>
              }
            >
              <PageCardBody>
                <ContributionHeatmap daily={data.daily} />
              </PageCardBody>
            </PageCard>
          )}

          {/* ---- Trends (single card, tabbed — no duplicate charts elsewhere) ---- */}
          {data.daily.length > 0 && (
            <PageCard
              title="Daily trends"
              description="Switch views — each tab shows a different slice of the same daily data."
              info={
                <InfoHover label="About daily trends">
                  <strong>Saved</strong>: tokens saved per day.
                  <strong> Reduction %</strong>: daily counterfactual vs response ratio.
                  <strong> Compare</strong>: side-by-side token volumes.
                  <strong> Weekday</strong>: savings aggregated by day of week (Mon–Sun).
                  <strong> Table</strong>: full numeric breakdown including cost.
                </InfoHover>
              }
            >
              <div className="sv-tab-bar">
                {(['saved', 'reduction', 'compare', 'weekday', 'table'] as const).map((v) => (
                  <button
                    key={v}
                    type="button"
                    className={`sv-tab${dailyView === v ? ' sv-tab--active' : ''}`}
                    onClick={() => setDailyView(v)}
                  >
                    {v === 'saved' ? 'Tokens saved' : v === 'reduction' ? 'Reduction %' : v === 'compare' ? 'Compare' : v === 'weekday' ? 'Weekday' : 'Table'}
                  </button>
                ))}
              </div>
              <PageCardBody>
                {dailyView === 'saved' && (
                  <AreaChart data={dailySavingsData} height={160} color="var(--ok)" />
                )}
                {dailyView === 'reduction' && (
                  <AreaChart data={dailyReductionData} height={160} color="var(--accent)" />
                )}
                {dailyView === 'compare' && <DailyBars daily={data.daily} />}
                {dailyView === 'weekday' && (
                  data.by_weekday.length > 0 ? (
                    <WeekdayBars rows={data.by_weekday} />
                  ) : (
                    <PageEmpty title="No weekday data">Graph MCP activity will populate weekday aggregates.</PageEmpty>
                  )
                )}
                {dailyView === 'table' && (
                  <DataTable>
                    <thead>
                      <tr>
                        <th>Date</th>
                        <th>Calls</th>
                        <th>Graph</th>
                        <th>Failed</th>
                        <th>Without ax</th>
                        <th>With ax</th>
                        <th>Saved</th>
                        <th>Cost saved</th>
                        <th>Reduction</th>
                        <th>Files</th>
                      </tr>
                    </thead>
                    <tbody>
                      {data.daily.map((d) => (
                        <tr key={d.date}>
                          <td>{d.date}</td>
                          <td className="num">{fmt(d.calls)}</td>
                          <td className="num">{fmt(d.graph_calls)}</td>
                          <td className="num">{d.failed_calls > 0 ? fmt(d.failed_calls) : '—'}</td>
                          <td className="num">{fmtCompact(d.counterfactual_tokens_est)}</td>
                          <td className="num">{fmtCompact(d.graph_response_tokens_est)}</td>
                          <td className="num">{fmtCompact(d.tokens_saved_est)}</td>
                          <td className="num">{fmtUsd(d.cost_saved_usd_est)}</td>
                          <td className="num">{dailyReductionPct(d)}%</td>
                          <td className="num">{fmt(d.counterfactual_files)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </DataTable>
                )}
              </PageCardBody>
            </PageCard>
          )}

          {/* ---- With vs Without + Efficiency (wide row) ---- */}
          <div className="sv-dashboard-row sv-dashboard-row--2col">
            <PageCard
              title="Context token comparison"
              description="Full file reads (counterfactual) vs graph responses (actual)."
              info={
                <InfoHover label="How this works">
                  <strong>Without ax</strong>: measured token count of every file a graph response referenced.
                  <strong> With ax</strong>: measured size of the graph responses themselves.
                </InfoHover>
              }
            >
              <PageCardBody>
                <ComparisonBars
                  withoutAx={data.counterfactual_tokens_est}
                  withAx={data.graph_response_tokens_est}
                  costWithout={data.counterfactual_cost_usd_est}
                  costWith={data.graph_response_cost_usd_est}
                />
              </PageCardBody>
            </PageCard>

            <PageCard
              title="Efficiency"
              description="Adoption, measurement quality, and utilization."
              info={
                <InfoHover label="About efficiency">
                  Gauges for graph-tool ratio, BPE measurement coverage, context reduction, and agent adoption from imported sessions.
                </InfoHover>
              }
            >
              <PageCardBody>
                <div className="sv-efficiency-grid">
                  <EfficiencyMetric
                    label="Exact file measurement"
                    value={data.counterfactual_exact_files}
                    maxVal={data.counterfactual_files}
                    detail={`${exactFilePct}% of file counterfactuals measured with BPE tokenizer`}
                  />
                  <EfficiencyMetric
                    label="Graph call volume"
                    value={data.graph_calls}
                    maxVal={Math.max(data.mcp_calls, 1)}
                    detail={`${fmt(data.graph_calls)} graph vs ${fmt(data.policy_calls)} policy calls`}
                  />
                  {data.clamp_tokens_absorbed !== 0 && (
                    <EfficiencyMetric
                      label="Clamp absorbed"
                      value={Math.abs(data.clamp_tokens_absorbed)}
                      maxVal={Math.max(data.tokens_saved_est, 1)}
                      detail="Difference between per-call clamp sum and aggregate net"
                    />
                  )}
                  {data.agent_sessions.length > 0 && (
                    <EfficiencyMetric
                      label="Session ax adoption"
                      value={totalAxCalls}
                      maxVal={totalAxCalls + totalReadCalls + totalGrepCalls}
                      detail={`${axShare}% ax vs Read/Grep in imported transcripts`}
                    />
                  )}
                </div>
              </PageCardBody>
            </PageCard>
          </div>

          {/* ---- By tool (single table — cards removed) ---- */}
          <PageCard
            title="By tool"
            description={`${toolGroups.graph.length} graph · ${toolGroups.policy.length} policy · ${toolGroups.other.length} other — full MCP audit.`}
            info={
              <InfoHover label="About this breakdown">
                One row per tool with counterfactual vs response tokens. Policy rows are dimmed — they log but do not save context.
              </InfoHover>
            }
          >
            <div className="sv-tab-bar sv-tab-bar--compact">
              {(['all', 'graph', 'policy'] as const).map((f) => (
                <button
                  key={f}
                  type="button"
                  className={`sv-tab${toolFilter === f ? ' sv-tab--active' : ''}`}
                  onClick={() => setToolFilter(f)}
                >
                  {f === 'all' ? `All (${data.by_tool.length})` : f === 'graph' ? `Graph (${toolGroups.graph.length})` : `Policy + other (${toolGroups.policy.length + toolGroups.other.length})`}
                </button>
              ))}
            </div>
            <PageCardBody>
              {filteredTools.length === 0 ? (
                <PageEmpty title="No MCP calls in this period">
                  Use ax MCP tools (ax_explore, ax_context, …) — each call is logged automatically.
                </PageEmpty>
              ) : (
                <ToolTable tools={filteredTools} totalSaved={data.tokens_saved_est} />
              )}
            </PageCardBody>
          </PageCard>

          {data.by_project.length > 0 && (
            <PageCard
              title="By project"
              description={`${data.projects_active} active projects in this period.`}
              info={<InfoHover label="About projects">Grouped by mcp_call_log.project path. Useful when you work across multiple repos.</InfoHover>}
            >
              <PageCardBody>
                <ProjectTable rows={data.by_project} />
              </PageCardBody>
            </PageCard>
          )}

          {data.recent_calls.length > 0 && (
            <PageCard
              title="Recent MCP calls"
              description="Last 40 calls in this period — live audit trail."
              info={<InfoHover label="About recent calls">Newest first. Failed calls highlighted. Only place with per-call timestamps.</InfoHover>}
            >
              <PageCardBody>
                <RecentCallsTable rows={data.recent_calls} />
              </PageCardBody>
            </PageCard>
          )}

          {/* ---- Agent sessions ---- */}
          <PageCard
            title="Agent sessions"
            description="Imported from local Cursor / Claude Code transcripts."
            info={
              <InfoHover label="About agent sessions">
                Independent evidence from your agents' own transcripts: how often each session
                used raw <strong>Read</strong>/<strong>Grep</strong> versus <strong>ax</strong> graph tools,
                plus the session's reported token usage and cost.
              </InfoHover>
            }
          >
            {data.agent_sessions.length > 0 && (
              <div className="sv-session-summary">
                <div className="sv-session-stat">
                  <span className="sv-session-stat-value">{data.agent_sessions.length}</span>
                  <span className="sv-session-stat-label">sessions</span>
                </div>
                <div className="sv-session-stat">
                  <span className="sv-session-stat-value">{fmtCompact(totalSessionTokens)}</span>
                  <span className="sv-session-stat-label">input tokens</span>
                </div>
                <div className="sv-session-stat">
                  <span className="sv-session-stat-value">{fmtUsd(totalSessionCost)}</span>
                  <span className="sv-session-stat-label">total cost</span>
                </div>
                <div className="sv-session-stat">
                  <span className="sv-session-stat-value">{totalAxCalls}</span>
                  <span className="sv-session-stat-label">ax calls</span>
                </div>
                <div className="sv-session-stat">
                  <span className="sv-session-stat-value">{totalReadCalls + totalGrepCalls}</span>
                  <span className="sv-session-stat-label">read + grep</span>
                </div>
                <div className="sv-session-stat">
                  <span className="sv-session-stat-value">{axShare}%</span>
                  <span className="sv-session-stat-label">ax adoption</span>
                </div>
              </div>
            )}
            <PageCardBody>
              <FilterBar>
                <span className="page-hint">Sessions imported from local Cursor / Claude Code transcripts.</span>
              </FilterBar>
              {data.agent_sessions.length === 0 ? (
                <PageEmpty title="No agent sessions imported yet">
                  Click Import sessions to scan local Cursor and Claude Code transcripts, or run <code>ax savings import --all</code>.
                </PageEmpty>
              ) : (
                <DataTable>
                  <thead>
                    <tr>
                      <th>Agent</th>
                      <th>Session</th>
                      <th>Model</th>
                      <th>Read</th>
                      <th>Grep</th>
                      <th>Ax</th>
                      <th>Input tokens</th>
                      <th>Cost</th>
                      <th>Saved in window</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.agent_sessions.map((s) => (
                      <tr key={`${s.agent}-${s.session_id}`}>
                        <td>{s.agent}</td>
                        <td className="mono">{s.session_id.slice(0, 8)}…</td>
                        <td className="mono">{s.model ?? '—'}</td>
                        <td className="num">{fmt(s.read_calls)}</td>
                        <td className="num">{fmt(s.grep_calls)}</td>
                        <td className="num">{fmt(s.ax_calls)}</td>
                        <td className="num">{s.session_input_tokens != null ? fmt(s.session_input_tokens) : '—'}</td>
                        <td className="num">{s.session_cost_usd_est != null ? fmtUsd(s.session_cost_usd_est) : '—'}</td>
                        <td className="num">{fmt(s.tokens_saved_in_window)}</td>
                      </tr>
                    ))}
                  </tbody>
                </DataTable>
              )}
            </PageCardBody>
          </PageCard>

          {/* ---- Methodology ---- */}
          <PageCard
            title="Methodology"
            description="How every number on this page is derived — deliberately conservative."
          >
            <div className="savings-method">
              <div className="savings-method-formula">
                {'saved(call) = max( Σ_files tokens(file contents) − tokens(response), 0 )'}
              </div>
              <p>
                Both sides are <strong>measured</strong> with the o200k BPE tokenizer: the response
                is tokenized directly, and each referenced file uses the{' '}
                <strong>{data.assumptions.counterfactual_mode}</strong> counterfactual baseline (
                <code>AX_SAVINGS_CF_MODE</code>: full file, symbol line span, or max). Unreadable
                files fall back to line heuristics, inline code-block content, then a{' '}
                {fmt(data.assumptions.avg_file_tokens)}-token average. In this period{' '}
                <strong>
                  {fmt(data.counterfactual_exact_files)} of {fmt(data.counterfactual_files)}
                </strong>{' '}
                file counterfactuals were measured exactly ({exactFilePct}%).
              </p>
              <p>
                The number is deliberately conservative: responses with{' '}
                <strong>no file references count as zero</strong> savings; the same file is counted{' '}
                <strong>once per call</strong>; per-call savings are <strong>clamped at zero</strong>;
                and <strong>failed calls and policy tools never contribute</strong>.
                {data.clamp_tokens_absorbed !== 0 && (
                  <>
                    {' '}The clamp absorbed <strong>{fmt(Math.abs(data.clamp_tokens_absorbed))}</strong> tokens this period;
                    unclamped net: <strong>{fmt(data.net_tokens_saved_est)}</strong>.
                  </>
                )}
              </p>
              <p>
                Dollar figures price saved tokens as input tokens at the{' '}
                <strong>{data.pricing.reference_model}</strong> reference rate (
                {fmtUsd(data.pricing.input_per_mtok)}/M in, {fmtUsd(data.pricing.output_per_mtok)}/M out).
                Session costs use each session's own model when known. Edit{' '}
                <code>{data.pricing.config_path}</code> to pin your own models and prices.
              </p>
              <div className="savings-method-grid">
                <div className="savings-method-item">
                  <span className="savings-method-item-label">Tokenizer</span>
                  <span className="savings-method-item-value">
                    {data.assumptions.exact_tokenizer ? 'o200k BPE (exact)' : 'heuristic fallback'}
                  </span>
                  <span className="savings-method-item-env">
                    {fmt(data.counterfactual_exact_files)}/{fmt(data.counterfactual_files)} files measured
                  </span>
                </div>
                <div className="savings-method-item">
                  <span className="savings-method-item-label">Pricing reference</span>
                  <span className="savings-method-item-value">{data.pricing.reference_model}</span>
                  <span className="savings-method-item-env">
                    {data.pricing.source === 'user' ? 'pricing.toml (user)' : 'built-in defaults'}
                  </span>
                </div>
                <div className="savings-method-item">
                  <span className="savings-method-item-label">Counterfactual mode</span>
                  <span className="savings-method-item-value">{data.assumptions.counterfactual_mode}</span>
                  <span className="savings-method-item-env">AX_SAVINGS_CF_MODE</span>
                </div>
                <div className="savings-method-item">
                  <span className="savings-method-item-label">Fallback tokens per line / file</span>
                  <span className="savings-method-item-value">
                    {data.assumptions.tokens_per_line} / {fmt(data.assumptions.avg_file_tokens)}
                  </span>
                  <span className="savings-method-item-env">AX_SAVINGS_TOKENS_PER_LINE</span>
                </div>
                <div className="savings-method-item">
                  <span className="savings-method-item-label">Log database</span>
                  <span className="savings-method-item-value" style={{ fontSize: '11px', wordBreak: 'break-all' }}>
                    {data.db_path}
                  </span>
                  <span className="savings-method-item-env">ax savings --json</span>
                </div>
              </div>
            </div>
          </PageCard>
        </div>
      )}
    </PageShell>
  );
}
