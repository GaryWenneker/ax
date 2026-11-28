/** A single cached promise that is reloaded once it is older than `maxAgeMs`, and never caches a failure. */
export interface RefreshingCache<T> {
  get(): Promise<T>;
  clear(): void;
}

export function createRefreshingCache<T>(
  load: () => Promise<T>,
  maxAgeMs: number,
  now: () => number = Date.now,
): RefreshingCache<T> {
  let entry: { at: number; value: Promise<T> } | null = null;
  return {
    get() {
      if (entry && now() - entry.at < maxAgeMs) return entry.value;
      const value: Promise<T> = load().catch((e: unknown) => {
        if (entry?.value === value) entry = null;
        throw e;
      });
      entry = { at: now(), value };
      return value;
    },
    clear() {
      entry = null;
    },
  };
}
