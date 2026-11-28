/** Pure state rules of the Specs & Plans page, kept out of the component so they can be unit tested. */

export type SpecsBladeTab = 'agreement' | 'plans' | 'authority';
export type SpecsKindFilter = 'all' | 'plan' | 'spec';
export type SpecsStatusFilter = 'all' | 'draft' | 'approved' | 'change_requested';

export interface VersionedRow {
  version: number;
  status?: string;
}

/**
 * The version to open after loading a document's revisions: the requested
 * version when it exists, else the approved one, else the newest.
 */
export function initialVersion(rows: readonly VersionedRow[], requested: number | null): number | null {
  if (requested != null && rows.some((row) => row.version === requested)) return requested;
  const approved = rows.find((row) => row.status === 'approved');
  if (approved) return approved.version;
  return rows.length > 0 ? rows[rows.length - 1].version : null;
}

/**
 * The query string that records the open spec, plan and blade tab. Other
 * parameters are kept. No spec clears all three; the default tab is left out.
 */
export function specRouteSearch(
  search: string,
  specId: string | null,
  planId: string | null,
  tab: SpecsBladeTab,
): string {
  const params = new URLSearchParams(search);
  if (!specId) {
    params.delete('spec');
    params.delete('plan');
    params.delete('tab');
  } else {
    params.set('spec', specId);
    if (planId) params.set('plan', planId);
    else params.delete('plan');
    if (tab === 'agreement') params.delete('tab');
    else params.set('tab', tab);
  }
  const out = params.toString();
  return out ? `?${out}` : '';
}

export interface SpecRouteTarget {
  specId: string;
  planId: string | null;
  tab: SpecsBladeTab;
}

/** The document a query string asks to open, or null when it names no spec. A plan always opens the Plans tab. */
export function specRouteTarget(search: string): SpecRouteTarget | null {
  const params = new URLSearchParams(search);
  const specId = params.get('spec');
  if (!specId) return null;
  const planId = params.get('plan') || null;
  const tabParam = params.get('tab');
  const tab: SpecsBladeTab = planId
    ? 'plans'
    : tabParam === 'plans' || tabParam === 'authority'
      ? tabParam
      : 'agreement';
  return { specId, planId, tab };
}

export interface FilterableSpec {
  id: string;
  title: string;
  path: string;
  sessions: string;
  kind: string;
  latestStatus: string;
}

/** The list rows that pass the kind and status filters and contain the search text (case-insensitive). */
export function filterSpecs<T extends FilterableSpec>(
  specs: readonly T[],
  kind: SpecsKindFilter,
  status: SpecsStatusFilter,
  query: string,
): T[] {
  const needle = query.trim().toLowerCase();
  return specs.filter((spec) => {
    if (kind !== 'all' && spec.kind !== kind) return false;
    if (status !== 'all' && spec.latestStatus !== status) return false;
    if (!needle) return true;
    return `${spec.title} ${spec.id} ${spec.path} ${spec.sessions}`.toLowerCase().includes(needle);
  });
}

export interface DiffSelection<T> {
  current: T | undefined;
  older: T[];
  base: T | null;
}

/**
 * The open revision, the revisions before it, and the one to diff against:
 * the chosen base when it is older than the open one, else the previous revision.
 */
export function diffSelection<T extends VersionedRow>(
  revisions: readonly T[],
  openVersion: number | null,
  baseChoice: number | null,
): DiffSelection<T> {
  const index = revisions.findIndex((row) => row.version === openVersion);
  const current = index >= 0 ? revisions[index] : undefined;
  const older = index > 0 ? revisions.slice(0, index) : [];
  const base = older.find((row) => row.version === baseChoice) ?? older[older.length - 1] ?? null;
  return { current, older, base };
}
