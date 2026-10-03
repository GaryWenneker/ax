export interface TraceGroupLink {
  key: string;
  hue: number;
  position: number;
  count: number;
  joinPrev: boolean;
  joinNext: boolean;
  lane: 'context' | 'token';
}

const CACHE_LINE =
  /(?:^|\s)cache\s+group=([A-Za-z0-9_-]{1,64})\s+lane=(context|token)\b/;

/** Group key of a cache status or store line, or null. */
export function cacheGroupKey(message: string): string | null {
  return message.match(CACHE_LINE)?.[1] ?? null;
}

export function cacheGroupLane(message: string): 'context' | 'token' | null {
  const lane = message.match(CACHE_LINE)?.[2];
  return lane === 'context' || lane === 'token' ? lane : null;
}

/** Stable hue in 0..359 from the group key. */
export function groupHue(key: string): number {
  let n = 0;
  for (let i = 0; i < key.length; i++) n = (Math.imul(n, 33) + key.charCodeAt(i)) >>> 0;
  return n % 360;
}

/** Rows are newest first. A group with one shown line is not linked. */
export function traceGroupLinks(messages: string[]): (TraceGroupLink | null)[] {
  const keys = messages.map(cacheGroupKey);
  const lanes = messages.map(cacheGroupLane);
  const counts = new Map<string, number>();
  for (const k of keys) if (k) counts.set(k, (counts.get(k) ?? 0) + 1);
  const seen = new Map<string, number>();
  return keys.map((key, i) => {
    const count = key ? counts.get(key) ?? 0 : 0;
    const lane = lanes[i];
    if (!key || !lane || count < 2) return null;
    const n = seen.get(key) ?? 0;
    seen.set(key, n + 1);
    return {
      key,
      hue: groupHue(key),
      position: count - n,
      count,
      joinPrev: keys[i - 1] === key,
      joinNext: keys[i + 1] === key,
      lane,
    };
  });
}
