export type SpecLink = { id: string; title: string; sessions: string };

export type SpecCatalog = { load: () => Promise<SpecLink[]>; invalidate: () => void };

/** How long a good spec list is reused before it is fetched again. */
export const SPEC_CATALOG_TTL_MS = 30_000;

/**
 * A shared, short-lived cache of `/api/specs`. Only a good answer is kept;
 * a failed or non-OK fetch yields `[]` and the next load tries again.
 */
export function createSpecCatalog(
  fetcher: (url: string) => Promise<Response> = (url) => fetch(url),
  now: () => number = Date.now,
  ttlMs: number = SPEC_CATALOG_TTL_MS,
): SpecCatalog {
  let cached: { at: number; specs: Promise<SpecLink[]> } | null = null;

  function load(): Promise<SpecLink[]> {
    if (cached && now() - cached.at <= ttlMs) return cached.specs;
    const entry = { at: now(), specs: Promise.resolve<SpecLink[]>([]) };
    entry.specs = fetcher('/api/specs')
      .then(async (response) => {
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const data = (await response.json()) as { specs?: SpecLink[] };
        return Array.isArray(data.specs) ? data.specs : [];
      })
      .catch(() => {
        if (cached === entry) cached = null;
        return [];
      });
    cached = entry;
    return entry.specs;
  }

  return {
    load,
    invalidate: () => {
      cached = null;
    },
  };
}
