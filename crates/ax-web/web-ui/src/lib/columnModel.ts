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

/**
 * One child of a column list: a data row, a group that holds rows (a session or turn), or a fixed
 * part such as a group heading that is never sorted or filtered.
 */
export type TreeEntry<R, G, F> =
  | {kind: 'row'; row: R}
  | {kind: 'group'; group: G; children: TreeEntry<R, G, F>[]}
  | {kind: 'fixed'; item: F};

/** Number of data rows in a tree, at any depth. */
export function countTreeRows<R, G, F>(entries: readonly TreeEntry<R, G, F>[]): number {
  return entries.reduce((total, entry) => total + (entry.kind === 'row' ? 1 : entry.kind === 'group' ? countTreeRows(entry.children) : 0), 0);
}

/**
 * Applies a column order (sort and filters) to rows at every depth. Rows are ordered among their
 * siblings, so they never leave their group; fixed parts keep their place, and a group whose rows
 * are all filtered out is dropped.
 */
export function arrangeTree<R, G, F>(entries: readonly TreeEntry<R, G, F>[], order: (rows: readonly R[]) => R[]): TreeEntry<R, G, F>[] {
  const ordered = order(entries.flatMap(entry => entry.kind === 'row' ? [entry.row] : []));
  let next = 0;
  const arranged: TreeEntry<R, G, F>[] = [];
  for (const entry of entries) {
    if (entry.kind === 'row') {
      if (next < ordered.length) arranged.push({kind: 'row', row: ordered[next++]});
    } else if (entry.kind === 'group') {
      const children = arrangeTree(entry.children, order);
      if (countTreeRows(children) > 0 || countTreeRows(entry.children) === 0) arranged.push({...entry, children});
    } else {
      arranged.push(entry);
    }
  }
  return arranged;
}
