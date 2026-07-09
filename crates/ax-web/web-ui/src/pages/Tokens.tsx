import { useCallback, useEffect, useState } from 'react';

import { fetchTokenUsage, type TokenUsageSummary } from '../api';
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
    <>
      <div className="page-header">
        <h1 className="page-title">Token usage</h1>
      </div>

      <p className="stats-summary">
        Per-model LLM tokens from explore offload. Stored in <code>~/.ax/usage.db</code>.
      </p>

      <div className="filter-row" style={{ marginBottom: '12px' }}>
        <select
          className="filter-select"
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
              className="filter-input"
              type="date"
              value={from}
              onChange={(e) => setFrom(e.target.value)}
              aria-label="From date"
              style={{ maxWidth: '160px' }}
            />
            <input
              className="filter-input"
              type="date"
              value={to}
              onChange={(e) => setTo(e.target.value)}
              aria-label="To date"
              style={{ maxWidth: '160px' }}
            />
          </>
        )}
        <button type="button" className="btn primary" onClick={() => void load()} disabled={loading}>
          {loading ? 'Loading…' : 'Apply'}
        </button>
      </div>

      {error && (
        <div className="state-msg">
          <strong>Error</strong>
          {error}
        </div>
      )}

      {!error && !data && loading && <div className="loading-row">Loading token usage…</div>}

      {data && (
        <>
          <table className="lang-table" style={{ marginBottom: '1.5rem' }}>
            <tbody>
              <tr>
                <th scope="row">Total tokens</th>
                <td style={{ fontVariantNumeric: 'tabular-nums' }}>{fmt(data.total_tokens)}</td>
                <th scope="row">Prompt</th>
                <td style={{ fontVariantNumeric: 'tabular-nums' }}>{fmt(data.prompt_tokens)}</td>
              </tr>
              <tr>
                <th scope="row">Completion</th>
                <td style={{ fontVariantNumeric: 'tabular-nums' }}>{fmt(data.completion_tokens)}</td>
                <th scope="row">Calls</th>
                <td style={{ fontVariantNumeric: 'tabular-nums' }}>{fmt(data.calls)}</td>
              </tr>
            </tbody>
          </table>

          <div style={{ marginTop: '1.5rem' }}>
            <div className="detail-section-title" style={{ marginBottom: '8px' }}>
              By model ({data.from} → {data.to})
            </div>
            {data.by_model.length === 0 ? (
              <p className="stats-summary">No usage in this period. Enable offload with <code>ax offload</code>.</p>
            ) : (
              <table className="lang-table">
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
                      <td style={{ fontFamily: 'var(--font-mono)', fontSize: 'var(--fs-sm)' }}>{row.model}</td>
                      <td style={{ fontVariantNumeric: 'tabular-nums' }}>{fmt(row.calls)}</td>
                      <td style={{ fontVariantNumeric: 'tabular-nums' }}>{fmt(row.prompt_tokens)}</td>
                      <td style={{ fontVariantNumeric: 'tabular-nums' }}>{fmt(row.completion_tokens)}</td>
                      <td style={{ fontVariantNumeric: 'tabular-nums' }}>{fmt(row.total_tokens)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>

          {data.daily.length > 0 && (
            <div style={{ marginTop: '1.5rem' }}>
              <div className="detail-section-title" style={{ marginBottom: '8px' }}>
                Daily usage
              </div>
              <table className="lang-table">
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
                      <td style={{ fontVariantNumeric: 'tabular-nums' }}>{fmt(d.calls)}</td>
                      <td style={{ fontVariantNumeric: 'tabular-nums' }}>{fmt(d.total_tokens)}</td>
                      <td>
                        <div
                          className="lang-bar"
                          style={{ width: `${Math.round((d.total_tokens / maxDaily) * 100)}%` }}
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </>
  );
}
