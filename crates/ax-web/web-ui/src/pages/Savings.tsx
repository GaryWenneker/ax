import { useCallback, useEffect, useState } from 'react';

import { fetchSavings, importSavingsSessions, type SavingsSummary } from '../api';
import { SavingsCompareChart, SavingsDailyCompareChart } from '../components/SavingsCompareChart';
import { InfoHover } from '../components/ui/InfoHover';
import {
  DataTable,
  DistBar,
  FilterBar,
  PageCard,
  PageCardBody,
  PageEmpty,
  PageHero,
  PageLoading,
  PageShell,
  PageStack,
  PageToasts,
  StatusPanel,
  StatusPill,
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

export default function SavingsPage() {
  const [period, setPeriod] = useState<Period>('month_to_date');
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const [data, setData] = useState<SavingsSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [importing, setImporting] = useState(false);
  const [importMsg, setImportMsg] = useState<string | null>(null);

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
    'Savings',
    data
      ? `${fmt(data.tokens_saved_est)} tok / ${fmtUsd(data.cost_saved_usd_est)} saved · ${data.from} → ${data.to}`
      : undefined,
  );

  const maxDaily = Math.max(...(data?.daily.map((d) => d.tokens_saved_est) ?? [1]), 1);
  const netPct = data ? pct(data.counterfactual_tokens_est - data.graph_response_tokens_est, data.counterfactual_tokens_est) : 0;
  const avgSavedPerCall = data && data.graph_calls > 0 ? Math.round(data.tokens_saved_est / data.graph_calls) : 0;
  const clampDelta = data ? data.tokens_saved_est - data.net_tokens_saved_est : 0;

  return (
    <PageShell>
      <PageHero
        title="Context savings"
        subtitle={
          <>
            Context tokens and dollars saved by answering from the code graph instead of reading
            full files. Responses are measured with the o200k BPE tokenizer; every MCP call is
            logged in <code>~/.ax/usage.db</code> — see Methodology below.
          </>
        }
      />

      <PageToasts err={error} />

      <PageStack>
        <PageCard
          title="Period"
          description="Select a time range for savings statistics."
          info={
            <InfoHover label="How periods work">
              <strong>Week</strong> = last 7 days, <strong>Month</strong> = last 30 days,{' '}
              <strong>Month to date</strong> and <strong>Year</strong> start on the 1st. Ranges use
              your local midnight; the log stores exact call timestamps.
            </InfoHover>
          }
        >
          <FilterBar>
            <select
              className="settings-select"
              value={period}
              onChange={(e) => setPeriod(e.target.value as Period)}
              aria-label="Time period"
            >
              {PERIOD_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
            {period === 'custom' && (
              <>
                <input
                  className="settings-input settings-input--narrow"
                  type="date"
                  value={from}
                  onChange={(e) => setFrom(e.target.value)}
                  aria-label="From date"
                />
                <input
                  className="settings-input settings-input--narrow"
                  type="date"
                  value={to}
                  onChange={(e) => setTo(e.target.value)}
                  aria-label="To date"
                />
              </>
            )}
            <button type="button" className="btn primary" onClick={() => void load()} disabled={loading}>
              {loading ? 'Loading…' : 'Apply'}
            </button>
          </FilterBar>
        </PageCard>

        {loading && !data && <PageLoading label="Loading savings…" />}

        {data && (
          <>
            <PageCard
              title="Summary"
              description={`${data.from} → ${data.to}`}
              info={
                <InfoHover label="How the summary is computed">
                  Only <strong>graph tools</strong> (ax_explore, ax_context, ax_node, ax_search,
                  ax_callers, ax_callees, ax_impact, ax_affected) count toward savings. Policy
                  tools and failed calls are logged but never claim savings.
                </InfoHover>
              }
            >
              <StatusPanel title="Totals">
                <StatusPill
                  label="Cost saved"
                  value={fmtUsd(data.cost_saved_usd_est)}
                  tone="ok"
                  info={
                    <InfoHover label="How cost saved is computed">
                      Tokens saved priced as <strong>input tokens</strong> at{' '}
                      <strong>{data.pricing.reference_model}</strong> rates (
                      {fmtUsd(data.pricing.input_per_mtok)}/M in). Override models and prices in{' '}
                      <code>{data.pricing.config_path}</code>
                      {data.pricing.source === 'user' ? ' (user pricing active)' : ''}.
                    </InfoHover>
                  }
                />
                <StatusPill
                  label="Tokens saved"
                  value={fmt(data.tokens_saved_est)}
                  tone="ok"
                  info={
                    <InfoHover label="How tokens saved is estimated">
                      Per call: <code>max(full-file estimate − actual response, 0)</code>, then
                      summed. The clamp means a verbose response never counts as negative savings —
                      the agent still got the information either way.
                      {clampDelta > 0 && (
                        <>
                          {' '}
                          In this period the clamp absorbed <strong>{fmt(clampDelta)}</strong>{' '}
                          tokens; the unclamped net difference is{' '}
                          <strong>{fmt(data.net_tokens_saved_est)}</strong>.
                        </>
                      )}
                    </InfoHover>
                  }
                />
                <StatusPill
                  label="Net reduction"
                  value={`${Math.max(netPct, 0)}%`}
                  tone={netPct > 0 ? 'ok' : 'neutral'}
                  info={
                    <InfoHover label="How net reduction is computed">
                      <code>(without ax − with ax) / without ax</code> over the whole period —
                      i.e. how much smaller the graph responses were than reading the referenced
                      files in full. This matches the comparison chart below exactly.
                    </InfoHover>
                  }
                />
                <StatusPill
                  label="Graph calls"
                  value={`${fmt(data.graph_calls)} / ${fmt(data.mcp_calls)}`}
                  info={
                    <InfoHover label="What graph calls means">
                      Graph-tool calls out of all logged MCP calls. The remainder are policy tools
                      (ax_preflight, ax_skill, …){data.failed_calls > 0 && (
                        <> and <strong>{fmt(data.failed_calls)}</strong> failed call(s)</>
                      )}{' '}
                      — logged for transparency, never counted as savings.
                    </InfoHover>
                  }
                />
                <StatusPill
                  label="File reads avoided"
                  value={fmt(data.counterfactual_files)}
                  info={
                    <InfoHover label="How file reads avoided is counted">
                      Distinct files referenced in each graph response. Without ax, the agent would
                      have opened these files to get the same facts. A file referenced by several
                      calls is counted once <strong>per call</strong>, because each turn would have
                      re-read it into context.
                    </InfoHover>
                  }
                />
                <StatusPill
                  label="With ax response"
                  value={`${fmt(data.graph_response_tokens_est)} tok`}
                  info={
                    <InfoHover label="How response size is measured">
                      {data.assumptions.exact_tokenizer ? (
                        <>
                          Graph response text tokenized with the <strong>o200k BPE</strong>{' '}
                          tokenizer — a measured value, not an estimate. This is what ax really put
                          into the agent's context — the "with ax" bar in the chart.
                        </>
                      ) : (
                        <>
                          Tokenizer unavailable — falling back to{' '}
                          <strong>{data.assumptions.chars_per_token} chars/token</strong>.
                        </>
                      )}
                    </InfoHover>
                  }
                />
                <StatusPill
                  label="Avg saved / call"
                  value={`~${fmt(avgSavedPerCall)} tok`}
                  info={
                    <InfoHover label="How the average is computed">
                      <code>tokens saved ÷ graph calls</code>. A quick sanity check: if this drops
                      toward zero, responses are nearly as large as the files they replace.
                    </InfoHover>
                  }
                />
              </StatusPanel>
            </PageCard>

            <PageCard
              title="With vs without ax"
              description="Estimated context tokens: reading full files vs ax graph MCP responses."
              info={
                <InfoHover label="How this comparison works">
                  <strong>Without ax</strong>: the measured token count of every file a graph
                  response referenced (tokenized file contents, with a line-based fallback for
                  unreadable files). <strong>With ax</strong>: the measured size of the graph
                  responses themselves. Same calls, two ways of getting the same facts.
                </InfoHover>
              }
            >
              <PageCardBody>
                <SavingsCompareChart
                  withoutAx={data.counterfactual_tokens_est}
                  withAx={data.graph_response_tokens_est}
                />
              </PageCardBody>
            </PageCard>

            <PageCard
              title="By tool"
              description="Breakdown per MCP graph tool."
              info={
                <InfoHover label="About this breakdown">
                  All logged tools appear here, but only graph tools accumulate savings. Policy
                  tools show 0 saved by design — they deliver rules, not code context.
                </InfoHover>
              }
            >
              <PageCardBody>
                {data.by_tool.length === 0 ? (
                  <PageEmpty title="No MCP calls in this period">
                    Use ax MCP tools (ax_explore, ax_context, …) — each call is logged automatically.
                  </PageEmpty>
                ) : (
                  <DataTable>
                    <thead>
                      <tr>
                        <th>Tool</th>
                        <th>Calls</th>
                        <th>
                          Tokens saved
                          <InfoHover label="How tokens saved per tool is computed">
                            Sum of per-call <code>max(counterfactual − response, 0)</code> for this
                            tool. Non-graph tools always show 0.
                          </InfoHover>
                        </th>
                        <th>
                          Files avoided
                          <InfoHover label="How files avoided is counted">
                            Distinct files referenced per call, summed over this tool's calls.
                          </InfoHover>
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {data.by_tool.map((row) => (
                        <tr key={row.tool}>
                          <td className="mono">{row.tool}</td>
                          <td className="num">{fmt(row.calls)}</td>
                          <td className="num">{fmt(row.tokens_saved_est)}</td>
                          <td className="num">{fmt(row.counterfactual_files)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </DataTable>
                )}
              </PageCardBody>
            </PageCard>

            {data.daily.length > 0 && (
              <PageCard
                title="Daily comparison"
                description="Without ax vs with ax per day."
                info={
                  <InfoHover label="About the daily chart">
                    Each pair is one local calendar day: orange = estimated full-file tokens,
                    green = actual graph response tokens. Days without graph calls are hidden.
                  </InfoHover>
                }
              >
                <PageCardBody>
                  <SavingsDailyCompareChart daily={data.daily} />
                </PageCardBody>
              </PageCard>
            )}

            {data.daily.length > 0 && (
              <PageCard
                title="Daily savings"
                description="Estimated tokens saved per day."
                info={
                  <InfoHover label="About this table">
                    Same per-call savings as the summary, grouped by local day.{' '}
                    <strong>Share</strong> scales each day against the best day in the period.
                  </InfoHover>
                }
              >
                <PageCardBody>
                  <DataTable>
                    <thead>
                      <tr>
                        <th>Date</th>
                        <th>Calls</th>
                        <th>Tokens saved</th>
                        <th style={{ width: '40%' }}>Share</th>
                      </tr>
                    </thead>
                    <tbody>
                      {data.daily.map((d) => (
                        <tr key={d.date}>
                          <td>{d.date}</td>
                          <td className="num">{fmt(d.calls)}</td>
                          <td className="num">{fmt(d.tokens_saved_est)}</td>
                          <td>
                            <DistBar pct={Math.round((d.tokens_saved_est / maxDaily) * 100)} />
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </DataTable>
                </PageCardBody>
              </PageCard>
            )}

            <PageCard
              title="Agent sessions"
              description="Imported from local Cursor / Claude Code transcripts."
              info={
                <InfoHover label="About agent sessions">
                  Independent evidence from your agents' own transcripts: how often each session
                  used raw <strong>Read</strong>/<strong>Grep</strong> versus <strong>ax</strong>{' '}
                  graph tools, plus the session's reported token usage and cost. More ax calls
                  relative to Read/Grep means the savings above are actually being realized.
                </InfoHover>
              }
            >
              <PageCardBody>
                <FilterBar>
                  <button
                    type="button"
                    className="btn"
                    onClick={() => void runImport()}
                    disabled={importing}
                  >
                    {importing ? 'Importing…' : 'Import sessions'}
                  </button>
                  {importMsg && <span className="page-hint">{importMsg}</span>}
                </FilterBar>
                {data.agent_sessions.length === 0 ? (
                  <PageEmpty title="No agent sessions imported yet">
                    Click Import sessions to scan local Cursor and Claude Code transcripts, or run{' '}
                    <code>ax savings import --all</code>.
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
                        <th>
                          Input tokens
                          <InfoHover label="What input tokens means">
                            Total input tokens the session reported (including cache reads), taken
                            from the agent's own usage records — not an ax estimate.
                          </InfoHover>
                        </th>
                        <th>
                          Cost
                          <InfoHover label="How session cost is computed">
                            Reported input tokens priced at the session's own model rate (or the
                            reference model when unknown).
                          </InfoHover>
                        </th>
                        <th>
                          Saved in window
                          <InfoHover label="What saved in window means">
                            Tokens saved by ax graph calls logged during this session's time
                            window — a per-session view of the totals above.
                          </InfoHover>
                        </th>
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
                          <td className="num">
                            {s.session_input_tokens != null ? fmt(s.session_input_tokens) : '—'}
                          </td>
                          <td className="num">
                            {s.session_cost_usd_est != null ? fmtUsd(s.session_cost_usd_est) : '—'}
                          </td>
                          <td className="num">{fmt(s.tokens_saved_in_window)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </DataTable>
                )}
              </PageCardBody>
            </PageCard>

            <PageCard
              title="Methodology"
              description="How every number on this page is derived — and why it errs on the low side."
            >
              <div className="savings-method">
                <div className="savings-method-formula">
                  {'saved(call) = max( Σ_files tokens(file contents) − tokens(response), 0 )'}
                </div>
                <p>
                  Both sides of the formula are <strong>measured</strong> with the o200k BPE
                  tokenizer: the response is tokenized directly, and each referenced file's actual
                  contents are tokenized (and cached) to compute what a full-file read would have
                  cost. Only when a file cannot be read does ax fall back to{' '}
                  <code>deepest line × {data.assumptions.tokens_per_line} tokens/line</code>, then
                  to a {fmt(data.assumptions.avg_file_tokens)}-token average. In this period{' '}
                  <strong>
                    {fmt(data.counterfactual_exact_files)} of {fmt(data.counterfactual_files)}
                  </strong>{' '}
                  file counterfactuals were measured exactly.
                </p>
                <p>
                  The number is deliberately conservative: responses with{' '}
                  <strong>no file references count as zero</strong> savings even though they
                  answered a question; the same file is counted <strong>once per call</strong> even
                  if referenced many times; per-call savings are <strong>clamped at zero</strong>;
                  and <strong>failed calls and policy tools never contribute</strong>.
                </p>
                <p>
                  Dollar figures price saved tokens as input tokens at the{' '}
                  <strong>{data.pricing.reference_model}</strong> reference rate (
                  {fmtUsd(data.pricing.input_per_mtok)}/M in, {fmtUsd(data.pricing.output_per_mtok)}
                  /M out). Session costs use each session's own model when known. Edit{' '}
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
          </>
        )}
      </PageStack>
    </PageShell>
  );
}
