/** Pure helpers for the online skill catalog search bar. */

export type CatalogHit = {
  id?: string;
  name: string;
  source: string;
  installs: number;
  install_url?: string | null;
  summary?: string | null;
  stars?: number | null;
  stars_label?: string | null;
  first_seen?: string | null;
  audits?: { name: string; status: string; url: string }[];
};

export type CatalogSettings = { reportInstalls: boolean; publishRemote: string };

/** One guard for both the Search button and the Enter key. */
export function canSearchCatalog(busy: boolean, query: string): boolean {
  return !busy && query.trim().length >= 2;
}

function optionalString(value: unknown): string | null | undefined {
  if (typeof value === 'string') return value;
  return value === null ? null : undefined;
}

/**
 * Parses a catalog search reply. A body that is not a JSON list throws; rows
 * without a name are dropped; a missing or non-numeric `installs` becomes 0.
 */
export function parseCatalogHits(text: string): CatalogHit[] {
  let body: unknown;
  try {
    body = JSON.parse(text);
  } catch {
    throw new Error('The skill catalog sent an unexpected reply.');
  }
  if (!Array.isArray(body)) throw new Error('The skill catalog sent an unexpected reply.');
  const hits: CatalogHit[] = [];
  for (const row of body) {
    if (!row || typeof row !== 'object') continue;
    const r = row as Record<string, unknown>;
    if (typeof r.name !== 'string' || !r.name) continue;
    const audits = Array.isArray(r.audits)
      ? r.audits.filter(
          (a): a is { name: string; status: string; url: string } =>
            !!a && typeof a === 'object' && typeof a.name === 'string' && typeof a.status === 'string' && typeof a.url === 'string',
        )
      : undefined;
    hits.push({
      id: typeof r.id === 'string' ? r.id : undefined,
      name: r.name,
      source: typeof r.source === 'string' ? r.source : '',
      installs: typeof r.installs === 'number' && Number.isFinite(r.installs) ? r.installs : 0,
      install_url: optionalString(r.install_url),
      summary: optionalString(r.summary),
      stars: typeof r.stars === 'number' && Number.isFinite(r.stars) ? r.stars : null,
      stars_label: optionalString(r.stars_label),
      first_seen: optionalString(r.first_seen),
      audits,
    });
  }
  return hits;
}

/** The settings to show once the save returns: the new ones on success, the old ones on failure. */
export function settingsAfterSave(before: CatalogSettings, wanted: CatalogSettings, ok: boolean): CatalogSettings {
  return ok ? wanted : before;
}
