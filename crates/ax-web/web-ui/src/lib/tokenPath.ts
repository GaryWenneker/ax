/** Pure path building and scaling for the token path graph. */

export interface PathPoint {
  position: number;
  token: string;
  weight: number;
}

export interface TokenPath {
  id: string;
  label: string;
  matched: boolean;
  points: PathPoint[];
  score: number;
}

function hashSeed(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

function mulberry32(seed: number): () => number {
  let a = seed || 1;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function freqMap(tokens: string[]): Map<string, number> {
  const m = new Map<string, number>();
  for (const t of tokens) m.set(t, (m.get(t) ?? 0) + 1);
  return m;
}

/** Relative weight per token (not model logits) — rarity + length + mild hash jitter.
 *  No position sine wave: that created a fake repeating zig-zag unrelated to time. */
function tokenWeight(token: string, _pos: number, freq: Map<string, number>): number {
  const f = freq.get(token) ?? 1;
  const rarity = 1 / f;
  const lenBoost = Math.min(Math.max(token.trim().length, 1), 16) / 16;
  const jitter = 0.88 + 0.24 * ((hashSeed(token) % 1000) / 1000);
  return Math.max(1e-4, (0.45 * rarity + 0.35 * lenBoost + 0.2) * jitter);
}

function pathScore(points: PathPoint[]): number {
  if (points.length === 0) return 0;
  return Math.exp(points.reduce((s, p) => s + Math.log(Math.max(p.weight, 1e-6)), 0) / points.length);
}

function buildMatchedPath(tokens: string[]): TokenPath {
  const freq = freqMap(tokens);
  const points = tokens.map((token, position) => ({
    position,
    token,
    weight: tokenWeight(token, position, freq),
  }));
  return {
    id: 'matched',
    label: 'Matched path (with ax)',
    matched: true,
    points,
    score: pathScore(points),
  };
}

function buildAltPaths(matched: string[], counterfactual: string[], altCount: number): TokenPath[] {
  const base = counterfactual.length > 0 ? counterfactual : matched;
  if (base.length === 0) return [];
  const paths: TokenPath[] = [];
  const len = Math.max(matched.length, Math.min(base.length, 120));
  const freq = freqMap(base);

  for (let i = 0; i < altCount; i++) {
    const rnd = mulberry32(hashSeed(`alt-${i}-${base.slice(0, 8).join('')}`));
    const points: PathPoint[] = [];
    // Offset so alts don't share the same modulo phase (looked like a repeating clock).
    const phase = Math.floor(rnd() * Math.max(base.length, 1));
    for (let p = 0; p < len; p++) {
      const src =
        base[(p + phase) % base.length] ?? matched[p % Math.max(matched.length, 1)] ?? '?';
      const tok = rnd() < 0.18 ? base[Math.floor(rnd() * base.length)] ?? src : src;
      const w = tokenWeight(tok, p, freq) * (0.35 + rnd() * 0.55);
      points.push({ position: p, token: tok, weight: Math.max(1e-4, w) });
    }
    paths.push({
      id: `alt-${i}`,
      label: `Alt path ${i + 1}`,
      matched: false,
      points,
      score: pathScore(points),
    });
  }
  return paths;
}

export function buildSyntheticPaths(
  responseTokens: number,
  counterfactualTokens: number,
  seed = 'synthetic',
  altCount = 48,
): { matched: TokenPath; alts: TokenPath[] } {
  const rnd = mulberry32(hashSeed(seed));
  const n = Math.max(8, Math.min(100, responseTokens || 40));
  const matchedTokens = Array.from({ length: n }, (_, i) => `t${i}`);
  const matched = buildMatchedPath(matchedTokens);
  const ratio =
    counterfactualTokens > 0
      ? Math.min(0.95, Math.max(0.15, responseTokens / counterfactualTokens))
      : 0.4;
  for (const p of matched.points) {
    // Smooth savings ratio envelope — no sine; jitter from RNG only.
    p.weight = Math.max(
      1e-4,
      (0.25 + (1 - ratio) * 0.7) * (0.55 + rnd() * 0.45),
    );
  }
  matched.score = pathScore(matched.points);
  const alts = buildAltPaths(
    matchedTokens,
    Array.from({ length: Math.min(120, Math.max(n, Math.round(n / Math.max(ratio, 0.1)))) }, (_, i) => `c${i}`),
    Math.max(0, Math.round(altCount)),
  );
  return { matched, alts };
}

/** Remap alt weights into the matched Y-band so grey paths stay visible (not crushed below). */
export function fitAltsIntoBand(alts: TokenPath[], minW: number, maxW: number): TokenPath[] {
  if (alts.length === 0) return alts;
  const bandLo = minW;
  const bandHi = Math.sqrt(Math.max(minW, 1e-9) * Math.max(maxW, minW * 1.01));
  return alts.map((alt) => {
    // A loop, not Math.min(...ws): spreading a long response overflows the call stack.
    let aLo = Infinity;
    let aHi = -Infinity;
    for (const p of alt.points) {
      if (p.weight < aLo) aLo = p.weight;
      if (p.weight > aHi) aHi = p.weight;
    }
    const span = Math.max(aHi - aLo, 1e-12);
    const rnd = mulberry32(hashSeed(alt.id));
    const ceiling = bandLo + (bandHi - bandLo) * (0.5 + 0.5 * rnd());
    return {
      ...alt,
      points: alt.points.map((p) => ({
        ...p,
        weight: bandLo + ((p.weight - aLo) / span) * Math.max(ceiling - bandLo, 1e-9),
      })),
    };
  });
}

export function buildPathsFromTokens(
  responseTokens: string[],
  counterfactualTokens: string[],
  altCount = 48,
): { matched: TokenPath; alts: TokenPath[] } {
  const matched = buildMatchedPath(
    responseTokens.length > 0 ? responseTokens : counterfactualTokens.slice(0, 40),
  );
  if (responseTokens.length === 0 && counterfactualTokens.length === 0) {
    return buildSyntheticPaths(40, 120, 'empty', altCount);
  }
  if (counterfactualTokens.length > responseTokens.length && responseTokens.length > 0) {
    const boost = Math.min(2.2, counterfactualTokens.length / responseTokens.length);
    for (const p of matched.points) p.weight = Math.min(0.99, p.weight * (0.7 + 0.15 * boost));
    matched.score = pathScore(matched.points);
  }
  return { matched, alts: buildAltPaths(responseTokens, counterfactualTokens, altCount) };
}

export function logY(w: number, minW: number, maxW: number, height: number): number {
  const lo = Math.log10(Math.max(minW, 1e-9));
  const hi = Math.log10(Math.max(maxW, minW * 1.01));
  const t = (Math.log10(Math.max(w, minW)) - lo) / Math.max(hi - lo, 1e-9);
  return height - Math.min(1, Math.max(0, t)) * height;
}

/** Fit log Y-range to the visible weight band (with padding), not a fixed 0.001→1 scale. */
export function weightBand(weights: number[]): { minW: number; maxW: number } {
  const sorted = weights.filter((w) => Number.isFinite(w) && w > 0).sort((a, b) => a - b);
  if (sorted.length === 0) return { minW: 0.05, maxW: 1 };

  // Drop extreme alt-path outliers so the matched band fills the plot.
  const loIdx = Math.floor(sorted.length * 0.05);
  const hiIdx = Math.min(sorted.length - 1, Math.ceil(sorted.length * 0.95) - 1);
  let lo = sorted[Math.max(0, loIdx)] ?? sorted[0];
  let hi = sorted[Math.max(loIdx, hiIdx)] ?? sorted[sorted.length - 1];
  if (hi <= lo) {
    lo = Math.max(1e-6, lo * 0.7);
    hi = hi * 1.4;
  }

  const logLo = Math.log10(lo);
  const logHi = Math.log10(hi);
  const pad = Math.max(0.04, (logHi - logLo) * 0.14);
  return {
    minW: 10 ** (logLo - pad),
    maxW: 10 ** (logHi + pad),
  };
}

export function formatWeightTick(v: number): string {
  if (v >= 1) return v >= 10 ? v.toFixed(0) : v.toFixed(1);
  if (v >= 0.1) return v.toFixed(2);
  if (v >= 0.01) return v.toFixed(3);
  return v.toExponential(0);
}

/** Evenly spaced ticks in log space within the fitted band. */
export function logTicks(minW: number, maxW: number, count = 4): number[] {
  const lo = Math.log10(Math.max(minW, 1e-9));
  const hi = Math.log10(Math.max(maxW, minW * 1.01));
  if (count < 2) return [maxW];
  return Array.from({ length: count }, (_, i) => 10 ** (lo + (i / (count - 1)) * (hi - lo)));
}

export function polyline(
  points: PathPoint[],
  width: number,
  height: number,
  minW: number,
  maxW: number,
  maxPos: number,
): string {
  if (points.length === 0) return '';
  return points
    .map((p) => {
      const x = (p.position / Math.max(maxPos, 1)) * width;
      const y = logY(p.weight, minW, maxW, height);
      return `${x},${y}`;
    })
    .join(' ');
}

export function histBins(weights: number[], minW: number, maxW: number, bins = 12): number[] {
  const counts = Array.from({ length: bins }, () => 0);
  if (weights.length === 0) return counts;
  const lo = Math.log10(Math.max(minW, 1e-9));
  const hi = Math.log10(Math.max(maxW, minW * 1.01));
  const span = Math.max(hi - lo, 1e-9);
  for (const w of weights) {
    const t = (Math.log10(Math.max(w, minW)) - lo) / span;
    const i = Math.min(bins - 1, Math.max(0, Math.floor(t * bins)));
    counts[i] += 1;
  }
  return counts;
}
