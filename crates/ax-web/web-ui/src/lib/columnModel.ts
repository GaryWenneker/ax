export type ColumnValue = string | number | boolean | null | undefined;
export interface ColumnSort { column: number; direction: 'asc' | 'desc' }
export const clampWidth = (value: number, minimum = 64) => Math.round(Math.max(minimum, Math.min(1600, value)));
export function validWidths(value: unknown, count: number, minimums: readonly number[] = []): number[] | null {
  return Array.isArray(value) && value.length === count && value.every((v, i) => typeof v === 'number' && Number.isFinite(v) && v >= (minimums[i] ?? 64) && v <= 1600) ? value : null;
}
function compare(a: ColumnValue, b: ColumnValue): number {
  if (typeof a === 'number' && typeof b === 'number') return a - b;
  const left = String(a ?? ''), right = String(b ?? '');
  const numeric = /^[$€£]?\s*-?\d+(?:,\d{3})*(?:\.\d+)?[kmb]?\s*%?$/i;
  if (numeric.test(left) && numeric.test(right)) {
    const value = (text: string) => {
      const clean = text.replace(/[$€£,%\s]/g, '');
      const suffix = clean.slice(-1).toLowerCase();
      const scale = suffix === 'k' ? 1e3 : suffix === 'm' ? 1e6 : suffix === 'b' ? 1e9 : 1;
      return Number(clean.replace(/[kmb]$/i, '')) * scale;
    };
    const x = value(left), y = value(right);
    if (Number.isFinite(x) && Number.isFinite(y)) return x - y;
  }
  return left.localeCompare(right, undefined, {numeric: true, sensitivity: 'base'});
}
export function orderRows<T extends {values: readonly ColumnValue[]}>(rows: readonly T[], sort: ColumnSort | null, filters: Record<number, string>): T[] {
  const matching = rows.map((row, index) => ({row, index})).filter(({row}) => Object.entries(filters).every(([column, query]) => String(row.values[Number(column)] ?? '').toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())));
  if (sort) matching.sort((a, b) => (sort.direction === 'asc' ? 1 : -1) * compare(a.row.values[sort.column], b.row.values[sort.column]) || a.index - b.index);
  return matching.map(({row}) => row);
}
