import type { DailySavingsRow } from '../api.ts';

export type HeatMetric = 'tokens' | 'calls' | 'graph';

export function parseLocalDate(iso: string): Date {
  const [y, m, d] = iso.split('-').map(Number);
  return new Date(y, (m ?? 1) - 1, d ?? 1);
}

export function toLocalIso(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

export function metricValue(row: DailySavingsRow | undefined, metric: HeatMetric): number {
  if (!row) return 0;
  if (metric === 'calls') return row.calls;
  if (metric === 'graph') return row.graph_calls;
  return row.tokens_saved_est;
}

export function computeStreaks(daily: DailySavingsRow[], metric: HeatMetric, year: number) {
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
      // Local midnights are 23 or 25 hours apart across a daylight-saving change.
      const diff = Math.round((d.getTime() - prev.getTime()) / 86400000);
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
