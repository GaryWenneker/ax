import { useCallback, useEffect, useState, type ReactNode } from 'react';

import {
  fetchCallTokenDetail,
  fetchSavings,
  importSavingsSessions,
  tokenizeText,
  type CallTokenDetail,
  type SavingsSummary,
  type DailySavingsRow,
  type ToolSavingsRow,
  type ProjectSavingsRow,
  type ModelSavingsRow,
  type RecentCallRow,
  type WeekdaySavingsRow,
  type HourSavingsRow,
  type TokenizeResult,
} from '../api';
import { computeInsights, dailyReductionPct, fmtTs, splitTools, toolCategory, type SavingsInsight } from './savingsMetrics';
import { InfoHover } from '../components/ui/InfoHover';
import { AbstractTokenRibbon, TokenChips } from '../components/TokenChips';
import { TokenPathGraph } from '../components/TokenPathGraph';
import { ActivityHeatmap } from '../components/ActivityHeatmap';
import { SavingsTimeline } from '../components/SavingsTimeline';
import { HoverTip } from '../components/HoverTip';
import { ModelNameCell } from '../components/ModelProviderIcon';
import { ResizableBlade } from '../components/BladeResize';
import Codicon from '../components/Codicon';
import {
  emptyQualitySnapshot,
  fetchMcpQuality,
  openMcpQualitySlideout,
  type QualitySnapshot,
} from '../lib/mcpQuality';
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

function ModelTable({ rows }: { rows: ModelSavingsRow[] }) {
  if (rows.length === 0) return null;
  return (
    <DataTable>
      <thead>
        <tr>
          <th>Model</th>
          <th>Sessions</th>
          <th>Input tokens</th>
          <th>Tokens saved</th>
          <th>Cost saved</th>
          <th>Session cost</th>
          <th>$/MTok</th>
          <th>Ax</th>
          <th>Read</th>
          <th>Grep</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((m) => (
          <tr key={m.model}>
            <td className="mono">
              <ModelNameCell model={m.model} />
            </td>
            <td className="num">{fmt(m.sessions)}</td>
            <td className="num">{fmtCompact(m.session_input_tokens)}</td>
            <td className="num">{m.tokens_saved_est > 0 ? fmtCompact(m.tokens_saved_est) : '—'}</td>
            <td className="num">{m.cost_saved_usd_est > 0 ? fmtUsd(m.cost_saved_usd_est) : '—'}</td>
            <td className="num">{m.session_cost_usd_est > 0 ? fmtUsd(m.session_cost_usd_est) : '—'}</td>
            <td className="num" title={m.pricing_source ?? undefined}>
              {m.input_per_mtok != null && m.input_per_mtok > 0
                ? `$${m.input_per_mtok.toFixed(2)}`
                : '—'}
            </td>
            <td className="num">{fmt(m.ax_calls)}</td>
            <td className="num">{fmt(m.read_calls)}</td>
            <td className="num">{fmt(m.grep_calls)}</td>
          </tr>
        ))}
      </tbody>
    </DataTable>
  );
}

function RecentCallsTable({
  rows,
  selectedId,
  onSelect,
}: {
  rows: RecentCallRow[];
  selectedId: number | null;
  onSelect: (row: RecentCallRow) => void;
}) {
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
          <tr
            key={`${r.id}-${r.created_at}-${i}`}
            className={[
              !r.ok ? 'sv-row-warn' : '',
              selectedId === r.id ? 'sv-row-selected' : '',
              'sv-row-clickable',
            ]
              .filter(Boolean)
              .join(' ')}
            onClick={() => onSelect(r)}
            title="Open token view"
          >
            <td className="mono">{fmtTs(r.created_at)}</td>
            <td className="mono">
              {r.tool}
              {r.has_preview && <span className="sv-preview-dot" title="Has token preview" />}
            </td>
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

function CallTokenBlade({
  detail,
  loading,
  error,
  onClose,
}: {
  detail: CallTokenDetail | null;
  loading: boolean;
  error: string | null;
  onClose: () => void;
}) {
  const savedPct =
    detail && detail.counterfactual_tokens_est > 0
      ? Math.round(
          ((detail.counterfactual_tokens_est - detail.response_tokens_est) /
            detail.counterfactual_tokens_est) *
            100,
        )
      : 0;

  return (
    <ResizableBlade>
      <div className="detail-panel detail-panel--blade" role="complementary" aria-label="Token view">
        <div className="detail-header">
          <span className="detail-title">
            <Codicon name="symbol-string" className="detail-title-icon" />
            {detail?.tool ?? 'Token view'}
          </span>
          <button type="button" className="detail-close" onClick={onClose} aria-label="Close">
            <Codicon name="close" />
          </button>
        </div>
        <div className="detail-body">
          {loading && <PageLoading label="Tokenizing…" />}
          {error && <PageEmpty title="Could not load call">{error}</PageEmpty>}
          {!loading && !error && detail && (
            <>
              <div className="detail-meta">
                <div className="detail-kv">
                  <span className="detail-key">When</span>
                  <span className="detail-val mono">{fmtTs(detail.created_at)}</span>
                </div>
                <div className="detail-kv">
                  <span className="detail-key">Saved</span>
                  <span className="detail-val">
                    {fmtCompact(detail.tokens_saved_est)} tok
                    {detail.counterfactual_tokens_est > 0 ? ` · ${savedPct}%` : ''}
                  </span>
                </div>
                <div className="detail-kv">
                  <span className="detail-key">Without / with</span>
                  <span className="detail-val">
                    {fmtCompact(detail.counterfactual_tokens_est)} / {fmtCompact(detail.response_tokens_est)}
                  </span>
                </div>
                <div className="detail-kv">
                  <span className="detail-key">Files</span>
                  <span className="detail-val">{detail.counterfactual_files || '—'}</span>
                </div>
              </div>

              <TokenPathGraph
                title="Call path graph"
                responseTokens={detail.response_tokens.tokens}
                counterfactualTokens={detail.counterfactual_tokens.tokens}
                responseTokenCount={detail.response_tokens_est}
                counterfactualTokenCount={detail.counterfactual_tokens_est}
              />

              <div className="sv-token-compare">
                <div className="sv-token-col">
                  <div className="detail-section-title">Without ax</div>
                  {detail.counterfactual_tokens.tokens.length > 0 ? (
                    <TokenChips
                      tokens={detail.counterfactual_tokens.tokens}
                      count={detail.counterfactual_tokens.count}
                      chars={detail.counterfactual_tokens.chars}
                      truncated={detail.counterfactual_tokens.truncated}
                    />
                  ) : (
                    <AbstractTokenRibbon
                      label="Full file reads"
                      tokens={detail.counterfactual_tokens_est}
                      tone="without"
                    />
                  )}
                </div>
                <div className="sv-token-col">
                  <div className="detail-section-title">With ax</div>
                  {detail.response_tokens.tokens.length > 0 ? (
                    <TokenChips
                      tokens={detail.response_tokens.tokens}
                      count={detail.response_tokens.count}
                      chars={detail.response_tokens.chars}
                      truncated={detail.response_tokens.truncated}
                    />
                  ) : (
                    <AbstractTokenRibbon
                      label="Graph response"
                      tokens={detail.response_tokens_est}
                      tone="with"
                    />
                  )}
                </div>
              </div>

              {!detail.response_preview && !detail.counterfactual_preview && (
                <p className="page-hint sv-token-hint">
                  Older calls have no stored text preview. New MCP graph calls store a truncated
                  o200k preview for this view.
                </p>
              )}
            </>
          )}
        </div>
      </div>
    </ResizableBlade>
  );
}

function TokenPlayground() {
  const [text, setText] = useState(
    'fn main() {\n    println!("hello from ax token view");\n}\n',
  );
  const [result, setResult] = useState<TokenizeResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const run = useCallback(async () => {
    setBusy(true);
    setErr(null);
    try {
      setResult(await tokenizeText(text));
    } catch (e) {
      setErr(String(e));
      setResult(null);
    } finally {
      setBusy(false);
    }
  }, [text]);

  return (
    <div className="sv-playground">
      <textarea
        className="sv-playground-input"
        rows={4}
        value={text}
        onChange={(e) => setText(e.target.value)}
        spellCheck={false}
        aria-label="Text to tokenize"
      />
      <div className="sv-playground-actions">
        <button type="button" className="btn primary" onClick={() => void run()} disabled={busy || !text.trim()}>
          {busy ? 'Tokenizing…' : 'Tokenize (o200k)'}
        </button>
        {err && <span className="page-hint sv-playground-err">{err}</span>}
      </div>
      {result && (
        <>
          <TokenPathGraph
            title="Playground path graph"
            responseTokens={result.tokens}
            counterfactualTokens={[]}
            responseTokenCount={result.count}
            counterfactualTokenCount={Math.round(result.count * 2.4)}
          />
          <TokenChips
            tokens={result.tokens}
            count={result.count}
            chars={result.chars}
            truncated={result.truncated}
          />
        </>
      )}
    </div>
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

function HourBars({ rows }: { rows: HourSavingsRow[] }) {
  const max = Math.max(...rows.map((r) => r.tokens_saved_est), 1);
  const peak = rows.reduce(
    (best, r) => (r.tokens_saved_est > best.tokens_saved_est ? r : best),
    rows[0] ?? { hour: 0, label: '00', tokens_saved_est: 0, calls: 0, graph_calls: 0 },
  );

  return (
    <div className="sv-hour-wrap">
      <p className="sv-hour-note">
        Local hour of day for MCP calls in this period
        {peak.tokens_saved_est > 0 ? (
          <>
            {' '}
            · peak <strong>{peak.label}:00</strong> ({fmtCompact(peak.tokens_saved_est)} tok)
          </>
        ) : null}
        . This is real clock time — not the token-path X axis.
      </p>
      <div className="sv-hour-bars" role="img" aria-label="Tokens saved by hour of day">
        {rows.map((r) => {
          const h = Math.round((r.tokens_saved_est / max) * 100);
          return (
            <HoverTip
              key={r.hour}
              tip={
                <>
                  <strong>{r.label}:00 – {r.label}:59</strong>
                  <span>{fmt(r.tokens_saved_est)} tokens saved</span>
                  <span>
                    {r.calls} calls · {r.graph_calls} graph
                  </span>
                </>
              }
            >
              <div className="sv-hour-col">
                <div className="sv-hour-bar-wrap">
                  <div
                    className={`sv-hour-bar${r.hour === peak.hour && peak.tokens_saved_est > 0 ? ' sv-hour-bar--peak' : ''}`}
                    style={{ height: `${Math.max(h, r.tokens_saved_est > 0 ? 6 : 0)}%` }}
                  />
                </div>
                {(r.hour % 3 === 0 || r.hour === 23) && (
                  <span className="sv-hour-label">{r.label}</span>
                )}
              </div>
            </HoverTip>
          );
        })}
      </div>
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
  const [dailyView, setDailyView] = useState<'saved' | 'reduction' | 'compare' | 'weekday' | 'hour' | 'table'>('saved');
  const [toolFilter, setToolFilter] = useState<'all' | 'graph' | 'policy'>('all');
  const [selectedCallId, setSelectedCallId] = useState<number | null>(null);
  const [callDetail, setCallDetail] = useState<CallTokenDetail | null>(null);
  const [callLoading, setCallLoading] = useState(false);
  const [callError, setCallError] = useState<string | null>(null);
  const [quality, setQuality] = useState<QualitySnapshot>(emptyQualitySnapshot);

  const openCallDetail = useCallback(async (row: RecentCallRow) => {
    if (selectedCallId === row.id) {
      setSelectedCallId(null);
      setCallDetail(null);
      setCallError(null);
      return;
    }
    setSelectedCallId(row.id);
    setCallLoading(true);
    setCallError(null);
    try {
      setCallDetail(await fetchCallTokenDetail(row.id));
    } catch (e) {
      setCallDetail(null);
      setCallError(String(e));
    } finally {
      setCallLoading(false);
    }
  }, [selectedCallId]);

  const closeCallDetail = useCallback(() => {
    setSelectedCallId(null);
    setCallDetail(null);
    setCallError(null);
  }, []);

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

  useEffect(() => {
    fetchMcpQuality()
      .then(setQuality)
      .catch(() => {});
  }, []);

  const runImport = useCallback(async () => {
    setImporting(true);
    setImportMsg(null);
    setError(null);
    try {
      const result = await importSavingsSessions();
      setImportMsg(
        `Imported ${result.claude_sessions} Claude + ${result.cursor_sessions} Cursor session(s)`,
      );
      await load();
    } catch (e) {
      setImportMsg(null);
      setError(e instanceof Error ? e.message : String(e));
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
            <button type="button" className="btn" onClick={() => void runImport()} disabled={importing} title="Import Cursor / Claude Code session transcripts">
              {importing ? <BusyLabel label="Importing…" /> : 'Import sessions'}
            </button>
            {loading && <span className="page-hint" aria-live="polite">Loading…</span>}
          </div>
        }
      />

      <button
        type="button"
        className="sv-quality-chip"
        onClick={() => openMcpQualitySlideout()}
        title="MCP quality score → savings reliability"
      >
        <span className="sv-quality-chip-score">
          Q {quality.score || '—'} {quality.grade !== '—' ? quality.grade : ''}
        </span>
        <span className="sv-quality-chip-meta">
          MCP quality → savings reliability
          {quality.tokensAtRisk > 0 ? ` · ~${quality.tokensAtRisk.toLocaleString()} tokens at risk` : ''}
        </span>
      </button>

      <PageToasts err={error} />
      {importMsg && <div className="settings-toast settings-toast--ok">{importMsg}</div>}

      {loading && !data && <PageLoading label="Loading savings…" />}

      {data && (
        <div className="sv-dashboard">
          <PageCard
            title="Savings over time"
            description="Hourly tokens saved in the selected period — navigate Older / Newer within that range."
            info={
              <InfoHover label="About the timeline">
                X axis is real local date and time. Y is tokens saved that hour, fitted to the
                visible window. Uses the same period filter as the rest of this page (selector above).
              </InfoHover>
            }
          >
            <PageCardBody>
              <SavingsTimeline timeline={data.timeline ?? []} />
            </PageCardBody>
          </PageCard>

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

          {/* ---- Activity heatmap (same period as filter) ---- */}
          <PageCard
            title="Activity"
            description="Full-year daily grid — empty days stay visible. Use ← → to browse other years."
            info={
              <InfoHover label="About the heatmap">
                GitHub-style calendar for the selected year. Each square is one day — grey means
                zero activity, green means more. Switch Tokens / All / Graph for intensity.
                Navigate years with the arrows; data is loaded per calendar year.
              </InfoHover>
            }
          >
            <PageCardBody>
              <ActivityHeatmap seedTo={data.to} />
              {(data.by_hour ?? []).some((h) => h.calls > 0) && (
                <div className="sv-hour-under-heat">
                  <h4 className="sv-hour-under-heat-title">By hour of day (selected period)</h4>
                  <HourBars rows={data.by_hour ?? []} />
                </div>
              )}
            </PageCardBody>
          </PageCard>

          <PageCard
            title="Token path graph"
            description={
              callDetail
                ? `Matched path for ${callDetail.tool} — green = with ax, grey = alternate context paths.`
                : 'Token-position illustration (not a timeline). Hover Alt / Matched for what those controls do.'
            }
            info={
              <InfoHover label="About the path graph">
                X is token index in the response stream — <strong>not</strong> clock time. Use{' '}
                <strong>Savings over time</strong> above for dates and hours. Alt = how many grey
                counterfactual paths to draw; Matched toggles the green path. Y scale fits the green
                path only.
              </InfoHover>
            }
          >
            <PageCardBody>
              <TokenPathGraph
                title={callDetail ? `${callDetail.tool} matched path` : 'Period path overview'}
                responseTokens={callDetail?.response_tokens.tokens ?? []}
                counterfactualTokens={callDetail?.counterfactual_tokens.tokens ?? []}
                responseTokenCount={
                  callDetail?.response_tokens_est ?? data.graph_response_tokens_est
                }
                counterfactualTokenCount={
                  callDetail?.counterfactual_tokens_est ?? data.counterfactual_tokens_est
                }
              />
            </PageCardBody>
          </PageCard>

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
                  <strong> Weekday</strong>: savings by day of week (Mon–Sun).
                  <strong> Hour</strong>: savings by local clock hour (00–23) — real time, not token-path position.
                  <strong> Table</strong>: full numeric breakdown including cost.
                </InfoHover>
              }
            >
              <div className="sv-tab-bar">
                {(['saved', 'reduction', 'compare', 'weekday', 'hour', 'table'] as const).map((v) => (
                  <button
                    key={v}
                    type="button"
                    className={`sv-tab${dailyView === v ? ' sv-tab--active' : ''}`}
                    onClick={() => setDailyView(v)}
                  >
                    {v === 'saved'
                      ? 'Tokens saved'
                      : v === 'reduction'
                        ? 'Reduction %'
                        : v === 'compare'
                          ? 'Compare'
                          : v === 'weekday'
                            ? 'Weekday'
                            : v === 'hour'
                              ? 'Hour'
                              : 'Table'}
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
                {dailyView === 'hour' && <HourBars rows={data.by_hour ?? []} />}
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

          {data.by_model.length > 0 && (
            <PageCard
              title="By model"
              description="Imported agent sessions grouped by model — savings attributed to each session's time window."
              info={
                <InfoHover label="About model breakdown">
                  From Cursor / Claude Code transcripts after <strong>Import sessions</strong>.
                  Cursor Composer rows get model + input tokens from{' '}
                  <code>state.vscdb</code> on import (context meter). Install the sessionStart hook (
                  <code>ax savings hook install</code>) if model still shows unknown.
                  Tokens saved are graph MCP savings during each session's start–end window.
                  Dollar columns use each model's price from <code>~/.ax/pricing.toml</code> when known.
                </InfoHover>
              }
            >
              <PageCardBody>
                <ModelTable rows={data.by_model} />
              </PageCardBody>
            </PageCard>
          )}

          {data.recent_calls.length > 0 && (
            <PageCard
              title="Recent MCP calls"
              description="Last 40 calls — click a row for o200k token chips (without vs with ax)."
              info={
                <InfoHover label="About recent calls">
                  Newest first. Failed calls highlighted. Click a row to open the token view —
                  color-coded o200k BPE chips from a truncated response / counterfactual preview.
                  Older calls without a stored preview show a count-only ribbon.
                </InfoHover>
              }
            >
              <PageCardBody>
                <div className={`page-split${selectedCallId != null ? ' page-split--with-detail' : ''}`}>
                  <div className="page-split-main">
                    <RecentCallsTable
                      rows={data.recent_calls}
                      selectedId={selectedCallId}
                      onSelect={(row) => void openCallDetail(row)}
                    />
                  </div>
                  {selectedCallId != null && (
                    <CallTokenBlade
                      detail={callDetail}
                      loading={callLoading}
                      error={callError}
                      onClose={closeCallDetail}
                    />
                  )}
                </div>
              </PageCardBody>
            </PageCard>
          )}

          <PageCard
            title="Token playground"
            description="Paste any text to see how o200k BPE splits it into tokens."
            info={
              <InfoHover label="About token playground">
                Same tokenizer ax uses for savings measurement (o200k_base). Useful to understand
                why a short string can still cost many tokens.
              </InfoHover>
            }
          >
            <PageCardBody>
              <TokenPlayground />
            </PageCardBody>
          </PageCard>

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
                        <td className="mono">
                          <ModelNameCell model={s.model ?? ''} />
                        </td>
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
                Session costs use each session's own model when known (see the <strong>Prices</strong> page
                for daily rate history). Edit <code>{data.pricing.config_path}</code> to pin overrides; synced
                OpenRouter rates fill in otherwise.
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
                    {data.pricing.source === 'user'
                      ? 'pricing.toml (user)'
                      : data.pricing.source === 'openrouter'
                        ? 'OpenRouter snapshot'
                        : data.pricing.source === 'artificial_analysis'
                          ? 'Artificial Analysis snapshot'
                          : 'built-in defaults'}
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
