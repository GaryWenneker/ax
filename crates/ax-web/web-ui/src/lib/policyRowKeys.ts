/**
 * Policy list rows are keyed by `rowKey` (origin + project + id), not by id:
 * a project rule and a global.db copy from another project can share an id.
 * Local rows carry no `rowKey`; their id (or skill name) is the key.
 */

export type KeyedPolicyRow = { rowKey?: string };

export function policyRowKey<T extends KeyedPolicyRow>(row: T, idOf: (row: T) => string): string {
  return row.rowKey ?? idOf(row);
}

export function indexPolicyRows<T extends KeyedPolicyRow>(rows: readonly T[], idOf: (row: T) => string): Map<string, T> {
  return new Map(rows.map((r) => [policyRowKey(r, idOf), r]));
}

/** `rows` with only the row under `key` replaced by `patch(row)`. */
export function patchPolicyRow<T extends KeyedPolicyRow>(
  rows: readonly T[],
  key: string,
  idOf: (row: T) => string,
  patch: (row: T) => T,
): T[] {
  return rows.map((r) => (policyRowKey(r, idOf) === key ? patch(r) : r));
}

/** `rows` without the row under `key`. */
export function dropPolicyRow<T extends KeyedPolicyRow>(rows: readonly T[], key: string, idOf: (row: T) => string): T[] {
  return rows.filter((r) => policyRowKey(r, idOf) !== key);
}
