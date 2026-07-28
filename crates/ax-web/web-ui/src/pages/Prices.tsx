import { useCallback, useEffect, useMemo, useState } from 'react';

import {
  fetchPricingCatalog,
  fetchPricingHistory,
  syncPricing,
  type PricingCatalogRow,
  type PricingHistoryPoint,
  type PricingStatus,
} from '../api';
import {
  BusyLabel,
  DataTable,
  FilterBar,
  PageCard,
  PageCardBody,
  PageEmpty,
  PageHero,
  PageLoading,
  PageShell,
  PageStack,
  PageToasts,
} from '../components/ui/PageLayout';
import { usePageContext } from '../context/UiContext';

function fmtUsd(n: number): string {
  if (!Number.isFinite(n)) return '—';
  if (n === 0) return '$0';
  if (n >= 100) return `$${n.toFixed(0)}`;
  if (n >= 10) return `$${n.toFixed(2)}`;
  if (n >= 1) return `$${n.toFixed(3)}`;
  return `$${n.toFixed(4)}`;
}

function RateChart({ points }: { points: PricingHistoryPoint[] }) {
  const series = points.filter((p) => p.source === 'openrouter');
  if (series.length < 2) {
    return (
      <p className="page-hint">
        Price-over-time appears after at least two daily syncs for this model.
      </p>
    );
  }

  const w = 640;
  const h = 200;
  const padL = 52;
  const padR = 16;
  const padT = 16;
  const padB = 28;
  const inputs = series.map((p) => p.input_per_mtok);
  const outputs = series.map((p) => p.output_per_mtok);
  const max = Math.max(...inputs, ...outputs, 0.01);
  const x = (i: number) => padL + (i / (series.length - 1)) * (w - padL - padR);
  const y = (v: number) => padT + ((max - v) / max) * (h - padT - padB);
  const line = (vals: number[]) =>
    vals.map((v, i) => `${i === 0 ? 'M' : 'L'} ${x(i).toFixed(1)} ${y(v).toFixed(1)}`).join(' ');
  const ticks = [max, max / 2, 0];

  return (
    <div className="prices-chart-wrap">
      <div className="prices-chart-legend">
        <span className="prices-chart-legend-item">
          <span className="prices-chart-swatch prices-chart-swatch--in" />
          Input $/MTok
        </span>
        <span className="prices-chart-legend-item">
          <span className="prices-chart-swatch prices-chart-swatch--out" />
          Output $/MTok
        </span>
      </div>
      <svg className="prices-chart" viewBox={`0 0 ${w} ${h}`} role="img" aria-label="Price over time">
        {ticks.map((t) => (
          <g key={`tick-${t}`}>
            <line
              x1={padL}
              x2={w - padR}
              y1={y(t)}
              y2={y(t)}
              stroke="var(--border)"
              strokeWidth="1"
            />
            <text x={padL - 6} y={y(t) + 3} fontSize="10" fill="var(--muted)" textAnchor="end">
              {fmtUsd(t)}
            </text>
          </g>
        ))}
        <path fill="none" stroke="var(--accent, #3b82f6)" strokeWidth="2.5" d={line(inputs)} />
        <path fill="none" stroke="var(--ok, #22c55e)" strokeWidth="2.5" d={line(outputs)} />
        {series.map((p, i) => (
          <g key={p.date}>
            <circle cx={x(i)} cy={y(p.input_per_mtok)} r="3.5" fill="var(--accent, #3b82f6)" />
            <circle cx={x(i)} cy={y(p.output_per_mtok)} r="3.5" fill="var(--ok, #22c55e)" />
          </g>
        ))}
        <text x={padL} y={h - 6} fontSize="10" fill="var(--muted)">
          {series[0].date}
        </text>
        <text x={w - padR} y={h - 6} fontSize="10" fill="var(--muted)" textAnchor="end">
          {series[series.length - 1].date}
        </text>
      </svg>
    </div>
  );
}

export default function PricesPage() {
  usePageContext('Prices', 'model rates');
  const [status, setStatus] = useState<PricingStatus | null>(null);
  const [models, setModels] = useState<PricingCatalogRow[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [history, setHistory] = useState<PricingHistoryPoint[]>([]);
  const [filter, setFilter] = useState('');
  const [busy, setBusy] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [toastErr, setToastErr] = useState<string | null>(null);

  const load = useCallback(async () => {
    setBusy(true);
    setErr(null);
    try {
      const catalog = await fetchPricingCatalog('openrouter');
      setStatus(catalog.status);
      setModels(catalog.models);
      setSelected((prev) => {
        if (prev && catalog.models.some((m) => m.model_id === prev)) return prev;
        return catalog.models[0]?.model_id ?? null;
      });
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!selected) {
      setHistory([]);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const rows = await fetchPricingHistory({
          model: selected,
          source: 'openrouter',
          days: 60,
        });
        if (!cancelled) setHistory(rows);
      } catch {
        if (!cancelled) setHistory([]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [selected]);

  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return models;
    return models.filter(
      (m) =>
        m.model_id.toLowerCase().includes(q) ||
        (m.display_name ?? '').toLowerCase().includes(q) ||
        (m.provider ?? '').toLowerCase().includes(q),
    );
  }, [models, filter]);

  async function onSync() {
    setSyncing(true);
    setErr(null);
    setToastErr(null);
    try {
      const report = await syncPricing(true);
      setToast(
        report.skipped
          ? 'Already synced today'
          : `Synced ${report.status}: ${report.openrouter_count} models`,
      );
      await load();
    } catch (e) {
      setErr(String(e));
      setToastErr(String(e));
    } finally {
      setSyncing(false);
    }
  }

  if (busy && !status) {
    return (
      <PageShell>
        <PageLoading label="Loading prices…" />
      </PageShell>
    );
  }

  return (
    <PageShell className="prices-page">
      <PageToasts ok={toast} err={toastErr} />
      <PageHero
        title="Model prices"
        subtitle="Daily OpenRouter rate snapshots power Savings cost estimates. Sync runs once per day when ax web or MCP starts."
        actions={
          <button type="button" className="btn primary" disabled={syncing} onClick={() => void onSync()}>
            {syncing ? <BusyLabel label="Syncing…" /> : 'Sync now'}
          </button>
        }
      />

      {err && <p className="page-hint">{err}</p>}

      <PageStack>
        {status && (
          <PageCard title="Sync status">
            <PageCardBody>
              <div className="prices-status">
                <span>
                  Today <strong>{status.today}</strong>
                </span>
                <span>
                  Synced today: <strong>{status.synced_today ? 'yes' : 'no'}</strong>
                </span>
                <span>
                  Models: <strong>{status.price_rows.toLocaleString()}</strong>
                </span>
              </div>
            </PageCardBody>
          </PageCard>
        )}

        <FilterBar>
          <input
            className="settings-input settings-input--grow"
            type="search"
            placeholder="Filter models…"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            aria-label="Filter models"
          />
        </FilterBar>

        {selected && (
          <PageCard
            title="Price over time"
            description="Input and output $/MTok for the selected model across daily syncs."
          >
            <PageCardBody>
              <div className="prices-chart-title mono">{selected}</div>
              <RateChart points={history} />
            </PageCardBody>
          </PageCard>
        )}

        {filtered.length === 0 ? (
          <PageEmpty title="No price snapshots yet">
            Click Sync now, or wait for the next daily auto-sync when ax web starts.
          </PageEmpty>
        ) : (
          <PageCard
            title="Models"
            description={`${filtered.length.toLocaleString()} OpenRouter models with current input/output rates.`}
            className="prices-list-card"
          >
            <PageCardBody>
              <div className="prices-table-scroll">
                <DataTable>
                  <thead>
                    <tr>
                      <th>Model</th>
                      <th>Input $/M</th>
                      <th>Output $/M</th>
                      <th>Date</th>
                    </tr>
                  </thead>
                  <tbody>
                    {filtered.map((m) => (
                      <tr
                        key={m.model_id}
                        className={selected === m.model_id ? 'is-selected' : undefined}
                        onClick={() => setSelected(m.model_id)}
                        style={{ cursor: 'pointer' }}
                      >
                        <td className="mono">{m.display_name || m.model_id}</td>
                        <td className="num">{fmtUsd(m.input_per_mtok)}</td>
                        <td className="num">{fmtUsd(m.output_per_mtok)}</td>
                        <td className="mono">{m.date}</td>
                      </tr>
                    ))}
                  </tbody>
                </DataTable>
              </div>
            </PageCardBody>
          </PageCard>
        )}
      </PageStack>
    </PageShell>
  );
}
