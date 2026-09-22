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
import {
  CHART_INPUT,
  CHART_OUTPUT,
  allZeroPrices,
  chartDomain,
  chartX,
  chartY,
  formatContextLength,
  mergeCatalog,
  pickDefaultModelId,
  providerLabel,
  providerSlug,
} from '../lib/pricesUi';

function fmtUsd(n: number): string {
  if (!Number.isFinite(n)) return '—';
  if (n === 0) return '$0';
  if (n >= 100) return `$${n.toFixed(0)}`;
  if (n >= 10) return `$${n.toFixed(2)}`;
  if (n >= 1) return `$${n.toFixed(3)}`;
  return `$${n.toFixed(4)}`;
}

function RateChart({ points }: { points: PricingHistoryPoint[] }) {
  const series = points.filter((p) => p.source === 'openrouter' || p.source === 'builtin');
  if (series.length < 1) {
    return (
      <p className="page-hint">
        Price-over-time appears after a daily sync for this model.
      </p>
    );
  }

  const w = 640;
  const h = 200;
  const padL = 52;
  const padR = 16;
  const padT = 16;
  const padB = 28;
  const innerW = w - padL - padR;
  const innerH = h - padT - padB;
  const inputs = series.map((p) => p.input_per_mtok);
  const outputs = series.map((p) => p.output_per_mtok);
  const zeros = allZeroPrices([...inputs, ...outputs]);
  const { min, max } = chartDomain([...inputs, ...outputs]);
  const overlap = !zeros && inputs.every((v, i) => v === outputs[i]);
  const x = (i: number) => chartX(i, series.length, padL, innerW);
  const yIn = (v: number) => chartY(v, min, max, padT, innerH) - (overlap || zeros ? 5 : 0);
  const yOut = (v: number) => chartY(v, min, max, padT, innerH) + (overlap || zeros ? 5 : 0);
  const line = (vals: number[], yFn: (v: number) => number) =>
    vals.map((v, i) => `${i === 0 ? 'M' : 'L'} ${x(i).toFixed(1)} ${yFn(v).toFixed(1)}`).join(' ');
  const ticks = zeros ? [0] : [max, (max + min) / 2, min];
  const yTick = (t: number) => chartY(t, min, max, padT, innerH);

  return (
    <div className="prices-chart-wrap">
      <div className="prices-chart-legend" role="list" aria-label="Chart legend">
        <span className="prices-chart-legend-item" role="listitem">
          <span className="prices-chart-swatch prices-chart-swatch--in" />
          Input $/MTok
        </span>
        <span className="prices-chart-legend-item" role="listitem">
          <span className="prices-chart-swatch prices-chart-swatch--out" />
          Output $/MTok
        </span>
      </div>
      {zeros ? (
        <p className="page-hint">
          This model is free: input and output are $0 / MTok. The chart sits on the zero line.
        </p>
      ) : null}
      <svg className="prices-chart" viewBox={`0 0 ${w} ${h}`} role="img" aria-label="Price over time">
        {ticks.map((t) => (
          <g key={`tick-${t}`}>
            <line
              x1={padL}
              x2={w - padR}
              y1={yTick(t)}
              y2={yTick(t)}
              stroke="var(--border)"
              strokeWidth="1"
            />
            <text x={padL - 6} y={yTick(t) + 3} fontSize="10" fill="var(--text-dim)" textAnchor="end">
              {fmtUsd(t)}
            </text>
          </g>
        ))}
        {series.length > 1 ? (
          <>
            <path fill="none" stroke={CHART_INPUT} strokeWidth="2.5" d={line(inputs, yIn)} />
            <path fill="none" stroke={CHART_OUTPUT} strokeWidth="2.5" d={line(outputs, yOut)} />
          </>
        ) : null}
        {series.map((p, i) => (
          <g key={`${p.date}-${i}`}>
            <circle cx={x(i)} cy={yIn(p.input_per_mtok)} r="3.5" fill={CHART_INPUT} />
            <circle cx={x(i)} cy={yOut(p.output_per_mtok)} r="3.5" fill={CHART_OUTPUT} />
          </g>
        ))}
        <text x={padL} y={h - 6} fontSize="10" fill="var(--text-dim)">
          {series[0].date || 'today'}
        </text>
        <text x={w - padR} y={h - 6} fontSize="10" fill="var(--text-dim)" textAnchor="end">
          {series[series.length - 1].date || 'today'}
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
      const models = mergeCatalog(catalog.models ?? []);
      setStatus(catalog.status);
      setModels(models);
      setSelected((prev) => {
        if (prev && models.some((m) => m.model_id === prev)) return prev;
        return pickDefaultModelId(models);
      });
    } catch (e) {
      setErr(String(e));
      const models = mergeCatalog([]);
      setModels(models);
      setSelected((prev) => prev ?? pickDefaultModelId(models));
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
        let rows = await fetchPricingHistory({
          model: selected,
          source: selected.startsWith('cursor/') ? undefined : 'openrouter',
          days: 60,
        });
        if (selected.startsWith('cursor/') && rows.length === 0) {
          const curated = mergeCatalog([]).find((m) => m.model_id === selected);
          if (curated) {
            rows = [
              {
                date: new Date().toISOString().slice(0, 10),
                source: 'builtin',
                input_per_mtok: curated.input_per_mtok,
                output_per_mtok: curated.output_per_mtok,
                blended_3_to_1: null,
              },
            ];
          }
        }
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

  const groupedByProvider = useMemo(() => {
    const map = new Map<string, PricingCatalogRow[]>();
    for (const row of filtered) {
      const key = providerSlug(row);
      const list = map.get(key) ?? [];
      list.push(row);
      map.set(key, list);
    }
    return [...map.entries()]
      .map(([provider, rows]) => {
        const inputs = rows.map((r) => r.input_per_mtok).filter((n) => Number.isFinite(n));
        const minIn = inputs.length ? Math.min(...inputs) : 0;
        return { provider, rows, minIn };
      })
      .sort((a, b) => a.provider.replace(/^~/, '').localeCompare(b.provider.replace(/^~/, '')));
  }, [filtered]);

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
            title="Models by provider"
            description={`${filtered.length.toLocaleString()} models (OpenRouter plus curated Cursor). Click a row for price over time.`}
            className="prices-list-card"
          >
            <PageCardBody>
              <nav className="prices-provider-legend" aria-label="Providers">
                {groupedByProvider.map((g) => (
                  <a key={g.provider} className="prices-provider-chip" href={`#prices-provider-${g.provider}`}>
                    {providerLabel(g.provider)}
                    <span>{g.rows.length}</span>
                  </a>
                ))}
              </nav>
              <div className="prices-provider-grid">
                {groupedByProvider.map((g) => (
                  <section
                    key={g.provider}
                    id={`prices-provider-${g.provider}`}
                    className="prices-provider-card"
                  >
                    <header className="prices-provider-head">
                      <h3>{providerLabel(g.provider)}</h3>
                      <p>
                        {g.rows.length} {g.rows.length === 1 ? 'model' : 'models'} · from {fmtUsd(g.minIn)}/MTok in
                      </p>
                    </header>
                    <div className="prices-table-scroll">
                      <DataTable>
                        <thead>
                          <tr>
                            <th>Model</th>
                            <th>Input $/M</th>
                            <th>Output $/M</th>
                            <th>Context</th>
                          </tr>
                        </thead>
                        <tbody>
                          {g.rows.map((m) => (
                            <tr
                              key={m.model_id}
                              className={selected === m.model_id ? 'is-selected' : undefined}
                              onClick={() => setSelected(m.model_id)}
                              style={{ cursor: 'pointer' }}
                            >
                              <td>{m.display_name || m.model_id}</td>
                              <td className="num">{fmtUsd(m.input_per_mtok)}</td>
                              <td className="num">{fmtUsd(m.output_per_mtok)}</td>
                              <td className="num">{formatContextLength(m.context_length)}</td>
                            </tr>
                          ))}
                        </tbody>
                      </DataTable>
                    </div>
                  </section>
                ))}
              </div>
            </PageCardBody>
          </PageCard>
        )}
      </PageStack>
    </PageShell>
  );
}
