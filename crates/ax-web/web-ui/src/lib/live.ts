/** Live updates: pure helpers (docs/specs/live-updates.md). */

export type LiveTopic = 'graph' | 'memory' | 'rules' | 'skills' | 'usage';

export const LIVE_TOPICS: readonly LiveTopic[] = ['graph', 'memory', 'rules', 'skills', 'usage'];

export type ChangeEvent = { topic: LiveTopic; version: string };

export function parseChange(data: string): ChangeEvent | null {
  let raw: unknown;
  try {
    raw = JSON.parse(data);
  } catch {
    return null;
  }
  if (typeof raw !== 'object' || raw === null) return null;
  const { topic, version } = raw as { topic?: unknown; version?: unknown };
  if (typeof version !== 'string') return null;
  if (!LIVE_TOPICS.includes(topic as LiveTopic)) return null;
  return { topic: topic as LiveTopic, version };
}

/** Keys in `after` that were not in `before`. `before === null` is a first load. */
export function diffNewKeys(before: readonly string[] | null, after: readonly string[]): string[] {
  if (before === null) return [];
  const seen = new Set(before);
  return after.filter((k) => !seen.has(k));
}

export type Scheduler = {
  set: (fn: () => void, ms: number) => unknown;
  clear: (handle: unknown) => void;
};

const browserScheduler: Scheduler = {
  set: (fn, ms) => setTimeout(fn, ms),
  clear: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
};

/** Collapse a burst of triggers into one call `ms` after the last trigger. */
export function createDebouncer(ms: number, fn: () => void, sched: Scheduler = browserScheduler): () => void {
  let handle: unknown = null;
  return () => {
    if (handle !== null) sched.clear(handle);
    handle = sched.set(() => {
      handle = null;
      fn();
    }, ms);
  };
}

export const SPARK_CAP = 20;

/** Sparks for at most `cap` nodes; a larger batch also gets one summary count. */
export function planSparks(newIds: readonly string[], cap = SPARK_CAP): { spark: string[]; summary: number | null } {
  return {
    spark: newIds.slice(0, cap),
    summary: newIds.length > cap ? newIds.length : null,
  };
}

export const SPARK_MS = 1200;

const SECOND_RING_DELAY_MS = 250;

function ring(elapsedMs: number): { grow: number; alpha: number } {
  const k = Math.min(1, Math.max(0, elapsedMs / (SPARK_MS - SECOND_RING_DELAY_MS)));
  const eased = 1 - (1 - k) ** 3;
  return { grow: eased, alpha: (1 - k) * 0.9 };
}

export function sparkPhase(elapsedMs: number): { rings: { grow: number; alpha: number }[]; core: number } | null {
  if (elapsedMs < 0 || elapsedMs > SPARK_MS) return null;
  return {
    rings: [ring(elapsedMs), ring(elapsedMs - SECOND_RING_DELAY_MS)],
    core: Math.max(0, 1 - elapsedMs / (SPARK_MS * 0.6)),
  };
}

export function edgeMarker(
  x: number,
  y: number,
  w: number,
  h: number,
  pad: number,
): { x: number; y: number; angle: number } | null {
  if (x >= 0 && x <= w && y >= 0 && y <= h) return null;
  return {
    x: Math.min(w - pad, Math.max(pad, x)),
    y: Math.min(h - pad, Math.max(pad, y)),
    angle: Math.atan2(y - h / 2, x - w / 2),
  };
}

export type KeyBaseline = { scope: string; keys: string[] };

export function trackNewKeys(
  prev: KeyBaseline | null,
  scope: string,
  keys: readonly string[],
): { state: KeyBaseline; added: string[] } {
  const state = { scope, keys: [...keys] };
  if (prev === null || prev.scope !== scope) return { state, added: [] };
  return { state, added: diffNewKeys(prev.keys, keys) };
}
