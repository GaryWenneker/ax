/** The Graph page's "As of" slider: a historical snapshot of the local graph. */

/** The slider spans the last 90 days. */
export const AS_OF_WINDOW_MS = 90 * 86_400_000;

/** Wait this long after the slider stops moving before loading a snapshot. */
export const AS_OF_COMMIT_MS = 300;

export type GraphView = {
  asOfMs: number | null;
  viewMode: 'structure' | 'domain';
  /** Replica system shown, or `''` for this project. */
  system: string;
};

/** Slider position 0–100 to a time; 100 (or more) is the live graph (`null`). */
export function asOfFromSlider(pct: number, now: number): number | null {
  if (pct >= 100) return null;
  const start = now - AS_OF_WINDOW_MS;
  return Math.floor(start + (Math.max(0, pct) / 100) * AS_OF_WINDOW_MS);
}

/** A time to its slider position; `null` (live) is 100. */
export function sliderFromAsOf(asOfMs: number | null, now: number): number {
  if (asOfMs == null) return 100;
  const pct = Math.round(((asOfMs - (now - AS_OF_WINDOW_MS)) / AS_OF_WINDOW_MS) * 100);
  return Math.min(100, Math.max(0, pct));
}

/**
 * Which load the page runs. Graph history covers the local graph only, so the
 * domain overlay and a replica system ignore the as-of time.
 */
export function graphSource(view: GraphView): 'domain' | 'as-of' | 'live' {
  if (view.viewMode === 'domain') return 'domain';
  if (view.asOfMs != null && view.system === '') return 'as-of';
  return 'live';
}

/** Live graph changes merge only into the live graph, never into a snapshot or the domain overlay. */
export function acceptsLiveMerge(view: GraphView): boolean {
  return graphSource(view) === 'live';
}
