import { useEffect, useMemo, useState } from 'react';

import { fetchSavings, type DailySavingsRow } from '../api';
import { HoverTip } from './HoverTip';

export type HeatMetric = 'tokens' | 'calls' | 'graph';

type DayKind = 'day' | 'pad' | 'future';

type DayCell = {
  date: string;
  value: number;
  level: number;
  kind: DayKind;
};

function parseLocalDate(iso: string): Date {
  const [y, m, d] = iso.split('-').map(Number);
  return new Date(y, (m ?? 1) - 1, d ?? 1);
}

function toLocalIso(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

function metricValue(row: DailySavingsRow | undefined, metric: HeatMetric): number {
  if (!row) return 0;
  if (metric === 'calls') return row.calls;
  if (metric === 'graph') return row.graph_calls;
  return row.tokens_saved_est;
}

function levelFor(value: number, max: number): number {
  if (value <= 0 || max <= 0) return 0;
  const t = value / max;
  if (t <= 0.25) return 1;
  if (t <= 0.5) return 2;
  if (t <= 0.75) return 3;
  return 4;
}

/** Full calendar year — Mon-aligned weeks from Jan 1 through Dec 31 (GitHub-style). */
function buildYearWeeks(
  daily: DailySavingsRow[],
  year: number,
  metric: HeatMetric,
): { weeks: DayCell[][]; max: number; total: number } {
  const map = new Map(daily.map((d) => [d.date, d]));
  const yearStart = `${year}-01-01`;
  const yearEnd = `${year}-12-31`;
  const today = toLocalIso(new Date());
  const currentYear = new Date().getFullYear();

  const values: number[] = [];
  for (const row of daily) {
    const iso = row.date;
    if (iso < yearStart || iso > yearEnd) continue;
    const v = metricValue(row, metric);
    if (v > 0) values.push(v);
  }
  const max = Math.max(1, ...values);
  let total = 0;

  const gridStart = parseLocalDate(yearStart);
  gridStart.setDate(gridStart.getDate() - ((gridStart.getDay() + 6) % 7));

  const gridEnd = parseLocalDate(yearEnd);
  gridEnd.setDate(gridEnd.getDate() + (6 - ((gridEnd.getDay() + 6) % 7)));

  const weeks: DayCell[][] = [];
  const cur = new Date(gridStart);
  let week: DayCell[] = [];

  while (cur <= gridEnd) {
    const iso = toLocalIso(cur);
    let kind: DayKind = 'day';
    let value = 0;
    let level = 0;

    if (iso < yearStart || iso > yearEnd) {
      kind = 'pad';
    } else if (year === currentYear && iso > today) {
      kind = 'future';
    } else {
      value = metricValue(map.get(iso), metric);
      total += value;
      level = levelFor(value, max);
    }

    week.push({ date: iso, value, level, kind });
    if (week.length === 7) {
      weeks.push(week);
      week = [];
    }
    cur.setDate(cur.getDate() + 1);
  }
  if (week.length > 0) weeks.push(week);

  return { weeks, max, total };
}

function computeStreaks(daily: DailySavingsRow[], metric: HeatMetric, year: number) {
  const yearStart = `${year}-01-01`;
  const yearEnd = `${year}-12-31`;
  const today = toLocalIso(new Date());
  const toIso = year === new Date().getFullYear() ? today : yearEnd;

  const map = new Map(
    daily
      .filter((d) => d.date >= yearStart && d.date <= yearEnd)
      .map((d) => [d.date, metricValue(d, metric)]),
  );
  const dates = [...map.keys()].sort();
  if (dates.length === 0) return { longest: 0, current: 0 };

  let longest = 0;
  let run = 0;
  let prev: Date | null = null;
  for (const iso of dates) {
    const v = map.get(iso) ?? 0;
    if (v <= 0) {
      run = 0;
      prev = null;
      continue;
    }
    const d = parseLocalDate(iso);
    if (prev) {
      const diff = (d.getTime() - prev.getTime()) / 86400000;
      run = diff === 1 ? run + 1 : 1;
    } else {
      run = 1;
    }
    longest = Math.max(longest, run);
    prev = d;
  }

  let current = 0;
  if (year === new Date().getFullYear()) {
    const cursor = parseLocalDate(toIso);
    let skippedToday = false;
    for (let i = 0; i < 400; i++) {
      const iso = toLocalIso(cursor);
      const v = map.get(iso) ?? 0;
      if (v <= 0) {
        if (!skippedToday && current === 0 && iso === toIso) {
          skippedToday = true;
          cursor.setDate(cursor.getDate() - 1);
          continue;
        }
        break;
      }
      current += 1;
      cursor.setDate(cursor.getDate() - 1);
    }
  }

  return { longest, current };
}

function mostActiveDay(daily: DailySavingsRow[], metric: HeatMetric, year: number) {
  const yearStart = `${year}-01-01`;
  const yearEnd = `${year}-12-31`;
  let best: { date: string; value: number } | null = null;
  for (const row of daily) {
    if (row.date < yearStart || row.date > yearEnd) continue;
    const value = metricValue(row, metric);
    if (!best || value > best.value) best = { date: row.date, value };
  }
  return best && best.value > 0 ? best : null;
}

function mostActiveMonth(daily: DailySavingsRow[], metric: HeatMetric, year: number) {
  const yearStart = `${year}-01-01`;
  const yearEnd = `${year}-12-31`;
  const byMonth = new Map<string, number>();
  for (const row of daily) {
    if (row.date < yearStart || row.date > yearEnd) continue;
    const key = row.date.slice(0, 7);
    byMonth.set(key, (byMonth.get(key) ?? 0) + metricValue(row, metric));
  }
  let bestKey = '';
  let bestVal = 0;
  for (const [k, v] of byMonth) {
    if (v > bestVal) {
      bestVal = v;
      bestKey = k;
    }
  }
  if (!bestKey || bestVal <= 0) return null;
  return parseLocalDate(`${bestKey}-01`).toLocaleString(undefined, { month: 'long' });
}

function formatDayLabel(iso: string): string {
  return parseLocalDate(iso).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
}

function metricLabel(metric: HeatMetric): string {
  if (metric === 'calls') return 'MCP calls';
  if (metric === 'graph') return 'Graph calls';
  return 'Tokens saved';
}

function formatValue(n: number, metric: HeatMetric): string {
  const s = n.toLocaleString();
  if (metric === 'tokens') return `${s} tokens`;
  if (metric === 'graph') return `${s} graph calls`;
  return `${s} calls`;
}

const METRIC_OPTIONS: Array<{ id: HeatMetric; label: string }> = [
  { id: 'tokens', label: 'Tokens' },
  { id: 'calls', label: 'All' },
  { id: 'graph', label: 'Graph' },
];

const WEEKDAY_LABELS = ['M', '', 'W', '', 'F', '', ''];

function initialYear(seedTo?: string): number {
  if (seedTo && seedTo.length >= 4) {
    const y = Number.parseInt(seedTo.slice(0, 4), 10);
    if (!Number.isNaN(y) && y >= 2000 && y <= 2100) return y;
  }
  return new Date().getFullYear();
}

export function ActivityHeatmap({ seedTo }: { seedTo?: string }) {
  const currentYear = new Date().getFullYear();
  const [viewYear, setViewYear] = useState(() => initialYear(seedTo));
  const [metric, setMetric] = useState<HeatMetric>('tokens');
  const [daily, setDaily] = useState<DailySavingsRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [fetchErr, setFetchErr] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setFetchErr(null);
    const from = `${viewYear}-01-01`;
    const to = viewYear >= currentYear ? toLocalIso(new Date()) : `${viewYear}-12-31`;
    fetchSavings({ period: 'custom', from, to })
      .then((r) => {
        if (!cancelled) setDaily(Array.isArray(r.daily) ? r.daily : []);
      })
      .catch((e: Error) => {
        if (!cancelled) {
          setFetchErr(e.message);
          setDaily([]);
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [viewYear, currentYear]);

  const { weeks, total } = useMemo(
    () => buildYearWeeks(daily, viewYear, metric),
    [daily, viewYear, metric],
  );
  const streaks = useMemo(() => computeStreaks(daily, metric, viewYear), [daily, metric, viewYear]);
  const topDay = useMemo(() => mostActiveDay(daily, metric, viewYear), [daily, metric, viewYear]);
  const topMonth = useMemo(() => mostActiveMonth(daily, metric, viewYear), [daily, metric, viewYear]);

  const months = useMemo(() => {
    const out: Array<{ label: string; col: number }> = [];
    let last = '';
    weeks.forEach((week, i) => {
      const first = week.find((d) => d.kind === 'day' || d.kind === 'future') ?? week[0];
      if (!first) return;
      const key = first.date.slice(0, 7);
      if (key === last) return;
      out.push({
        label: parseLocalDate(first.date).toLocaleString(undefined, { month: 'narrow' }),
        col: i,
      });
      last = key;
    });
    return out;
  }, [weeks]);

  if (weeks.length === 0) return null;

  const colTemplate = `repeat(${weeks.length}, var(--sv-hm-cell))`;
  const canGoForward = viewYear < currentYear;

  function cellClass(day: DayCell): string {
    if (day.kind === 'pad') return 'sv-hm-cell sv-hm-cell--pad';
    if (day.kind === 'future') return 'sv-hm-cell sv-hm-cell--future';
    return `sv-hm-cell sv-hm-cell--${day.level}`;
  }

  return (
    <div className="sv-hm">
      <div className="sv-hm-header">
        <div className="sv-hm-title-block">
          <span className="sv-hm-eyebrow">
            {metricLabel(metric)} · {viewYear}
            {loading ? ' · loading…' : ''}
          </span>
          <span className="sv-hm-total">{total.toLocaleString()}</span>
        </div>
        <div className="sv-hm-header-right">
          <div className="sv-hm-year-nav" aria-label="Year">
            <button
              type="button"
              className="sv-hm-year-btn"
              aria-label="Previous year"
              onClick={() => setViewYear((y) => y - 1)}
            >
              ←
            </button>
            <span className="sv-hm-year-label">{viewYear}</span>
            <button
              type="button"
              className="sv-hm-year-btn"
              aria-label="Next year"
              disabled={!canGoForward}
              onClick={() => setViewYear((y) => y + 1)}
            >
              →
            </button>
          </div>
          <div className="sv-hm-filters" role="tablist" aria-label="Heatmap metric">
            {METRIC_OPTIONS.map((opt) => (
              <button
                key={opt.id}
                type="button"
                role="tab"
                aria-selected={metric === opt.id}
                className={`sv-hm-filter${metric === opt.id ? ' sv-hm-filter--active' : ''}`}
                onClick={() => setMetric(opt.id)}
              >
                {opt.label}
              </button>
            ))}
          </div>
        </div>
      </div>

      {fetchErr && <p className="sv-hm-error">{fetchErr}</p>}

      <div className="sv-hm-body">
        <div className="sv-hm-ydays" aria-hidden>
          {WEEKDAY_LABELS.map((label, i) => (
            <span key={i} className="sv-hm-yday">
              {label}
            </span>
          ))}
        </div>
        <div className="sv-hm-scroll">
          <div className="sv-hm-months" style={{ gridTemplateColumns: colTemplate }}>
            {months.map((m) => (
              <span key={`${m.label}-${m.col}`} className="sv-hm-month" style={{ gridColumn: m.col + 1 }}>
                {m.label}
              </span>
            ))}
          </div>
          <div className="sv-hm-grid" style={{ gridTemplateColumns: colTemplate }}>
            {weeks.map((week, wi) =>
              week.map((day, di) => {
                const place = { gridColumn: wi + 1, gridRow: di + 1 } as const;
                const cls = cellClass(day);

                if (day.kind === 'pad') {
                  return (
                    <span key={day.date} style={place} className="sv-hm-cell-slot">
                      <span className={cls} aria-hidden />
                    </span>
                  );
                }

                if (day.kind === 'future') {
                  return (
                    <span key={day.date} style={place} className="sv-hm-cell-slot">
                      <span className={cls} aria-hidden title={formatDayLabel(day.date)} />
                    </span>
                  );
                }

                return (
                  <HoverTip
                    key={day.date}
                    className="sv-hm-cell-slot"
                    style={place}
                    tip={
                      <>
                        <strong>{formatDayLabel(day.date)}</strong>
                        <span>{formatValue(day.value, metric)}</span>
                        {day.value === 0 && <span>No activity this day</span>}
                      </>
                    }
                  >
                    <button
                      type="button"
                      className={cls}
                      aria-label={`${formatDayLabel(day.date)}: ${formatValue(day.value, metric)}`}
                    />
                  </HoverTip>
                );
              }),
            )}
          </div>
        </div>
      </div>

      <div className="sv-hm-footer">
        <div className="sv-hm-legend">
          <span>Fewer</span>
          {[0, 1, 2, 3, 4].map((l) => (
            <HoverTip
              key={l}
              tip={
                <>
                  <strong>Intensity {l}</strong>
                  <span>
                    {l === 0
                      ? 'No tokens / calls that day'
                      : l === 4
                        ? 'Top quartile of daily activity'
                        : `Level ${l} of 4 vs the peak day in ${viewYear}`}
                  </span>
                </>
              }
            >
              <span className={`sv-hm-cell sv-hm-cell--${l} sv-hm-cell--legend`} />
            </HoverTip>
          ))}
          <span>More</span>
        </div>
        <dl className="sv-hm-kpis">
          <HoverTip tip={<><strong>Most active month</strong><span>Month with the highest sum of the selected metric in {viewYear}.</span></>}>
            <div>
              <dt>Most active month</dt>
              <dd>{topMonth ?? '—'}</dd>
            </div>
          </HoverTip>
          <HoverTip
            tip={
              <>
                <strong>Most active day</strong>
                <span>
                  {topDay
                    ? `${formatDayLabel(topDay.date)} · ${formatValue(topDay.value, metric)}`
                    : `No active days in ${viewYear}`}
                </span>
              </>
            }
          >
            <div>
              <dt>Most active day</dt>
              <dd>{topDay ? formatDayLabel(topDay.date) : '—'}</dd>
            </div>
          </HoverTip>
          <HoverTip tip={<><strong>Longest streak</strong><span>Longest run of consecutive days with activity &gt; 0 in {viewYear}.</span></>}>
            <div>
              <dt>Longest streak</dt>
              <dd>{streaks.longest > 0 ? `${streaks.longest}d` : '—'}</dd>
            </div>
          </HoverTip>
          <HoverTip tip={<><strong>Current streak</strong><span>Consecutive active days ending today — only shown for the current year.</span></>}>
            <div>
              <dt>Current streak</dt>
              <dd>{viewYear === currentYear && streaks.current > 0 ? `${streaks.current}d` : '—'}</dd>
            </div>
          </HoverTip>
        </dl>
      </div>
    </div>
  );
}
