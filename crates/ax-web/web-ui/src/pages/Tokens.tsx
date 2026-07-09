import { useCallback, useEffect, useState } from 'react';

import { fetchTokenUsage, type TokenUsageSummary } from '../api';
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

export default function TokensPage() {
  const [period, setPeriod] = useState<Period>('month_to_date');
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const [data, setData] = useState<TokenUsageSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    if (period === 'custom' && !from) {
      setError('Pick a start date for custom range.');
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const summary = await fetchTokenUsage({
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

  usePageContext(
    'Tokens',
    data ? `${fmt(data.total_tokens)} tokens · ${data.from} → ${data.to}` : undefined,
  );

  const maxDaily = Math.max(...(data?.daily.map((d) => d.total_tokens) ?? [1]), 1);

  return (
    <PageShell>
      <PageHero
        title="Token usage"
        subtitle={
          <>
            Per-model LLM tokens from explore offload. Stored in <code>~/.ax/usage.db</code>.
          </>
        }
      />

      <PageToasts err={error} />

      <PageStack>
        <PageCard title="Period" description="Select a time range for usage statistics.">
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

        {loading && !data && <PageLoading label="Loading token usage…" />}

        {data && (
          <>
            <PageCard title="Summary" description={`${data.from} → ${data.to}`}>
              <StatusPanel title="Totals">
                <StatusPill label="Total tokens" value={fmt(data.total_tokens)} tone="ok" />
                <StatusPill label="Prompt" value={fmt(data.prompt_tokens)} />
                <StatusPill label="Completion" value={fmt(data.completion_tokens)} />
                <StatusPill label="Calls" value={fmt(data.calls)} />
              </StatusPanel>
            </PageCard>

            <PageCard title="By model" description="Breakdown per LLM model.">
              <PageCardBody>
                {data.by_model.length === 0 ? (
                  <PageEmpty title="No usage in this period">
                    Enable offload with <code>ax offload</code>.
                  </PageEmpty>
                ) : (
                  <DataTable>
                    <thead>
                      <tr>
                        <th>Model</th>
                        <th>Calls</th>
                        <th>Prompt</th>
                        <th>Completion</th>
                        <th>Total</th>
                      </tr>
                    </thead>
                    <tbody>
                      {data.by_model.map((row) => (
                        <tr key={row.model}>
                          <td className="mono">{row.model}</td>
                          <td className="num">{fmt(row.calls)}</td>
                          <td className="num">{fmt(row.prompt_tokens)}</td>
                          <td className="num">{fmt(row.completion_tokens)}</td>
                          <td className="num">{fmt(row.total_tokens)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </DataTable>
                )}
              </PageCardBody>
            </PageCard>

            {data.daily.length > 0 && (
              <PageCard title="Daily usage" description="Token volume per day.">
                <PageCardBody>
                  <DataTable>
                    <thead>
                      <tr>
                        <th>Date</th>
                        <th>Calls</th>
                        <th>Tokens</th>
                        <th style={{ width: '40%' }}>Share</th>
                      </tr>
                    </thead>
                    <tbody>
                      {data.daily.map((d) => (
                        <tr key={d.date}>
                          <td>{d.date}</td>
                          <td className="num">{fmt(d.calls)}</td>
                          <td className="num">{fmt(d.total_tokens)}</td>
                          <td>
                            <DistBar pct={Math.round((d.total_tokens / maxDaily) * 100)} />
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </DataTable>
                </PageCardBody>
              </PageCard>
            )}
          </>
        )}
      </PageStack>
    </PageShell>
  );
}
