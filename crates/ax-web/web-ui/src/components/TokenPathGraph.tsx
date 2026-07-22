import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { HoverTip } from './HoverTip';

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
function fitAltsIntoBand(alts: TokenPath[], minW: number, maxW: number): TokenPath[] {
  if (alts.length === 0) return alts;
  const bandLo = minW;
  const bandHi = Math.sqrt(Math.max(minW, 1e-9) * Math.max(maxW, minW * 1.01));
  return alts.map((alt) => {
    const ws = alt.points.map((p) => p.weight);
    const aLo = Math.min(...ws);
    const aHi = Math.max(...ws);
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

function logY(w: number, minW: number, maxW: number, height: number): number {
  const lo = Math.log10(Math.max(minW, 1e-9));
  const hi = Math.log10(Math.max(maxW, minW * 1.01));
  const t = (Math.log10(Math.max(w, minW)) - lo) / Math.max(hi - lo, 1e-9);
  return height - Math.min(1, Math.max(0, t)) * height;
}

/** Fit log Y-range to the visible weight band (with padding), not a fixed 0.001→1 scale. */
function weightBand(weights: number[]): { minW: number; maxW: number } {
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

function formatWeightTick(v: number): string {
  if (v >= 1) return v >= 10 ? v.toFixed(0) : v.toFixed(1);
  if (v >= 0.1) return v.toFixed(2);
  if (v >= 0.01) return v.toFixed(3);
  return v.toExponential(0);
}

/** Evenly spaced ticks in log space within the fitted band. */
function logTicks(minW: number, maxW: number, count = 4): number[] {
  const lo = Math.log10(Math.max(minW, 1e-9));
  const hi = Math.log10(Math.max(maxW, minW * 1.01));
  if (count < 2) return [maxW];
  return Array.from({ length: count }, (_, i) => 10 ** (lo + (i / (count - 1)) * (hi - lo)));
}

function polyline(
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

function histBins(weights: number[], minW: number, maxW: number, bins = 12): number[] {
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

type TipState = {
  point: PathPoint;
  clientX: number;
  clientY: number;
  pinned: boolean;
  place: 'above' | 'below';
};

const LONG_PRESS_MS = 420;
const PLOT_H = 240;

function formatTokenLabel(token: string, position: number): string {
  const t = token.trim();
  if (!t || t === '·' || /^t\d+$/.test(t)) {
    return `token #${position}`;
  }
  if (/^c\d+$/.test(t)) {
    return `alt #${position}`;
  }
  // Visible whitespace / control chars
  const shown = token
    .replace(/\n/g, '\\n')
    .replace(/\t/g, '\\t')
    .replace(/ /g, '·');
  return shown.length > 48 ? `${shown.slice(0, 45)}…` : shown;
}

function formatPreview(points: PathPoint[], pos: number): string {
  const start = Math.max(0, pos - 3);
  const end = Math.min(points.length, pos + 4);
  const parts = points.slice(start, end).map((p, i) => {
    const label = formatTokenLabel(p.token, p.position);
    return start + i === pos ? `⟦${label}⟧` : label;
  });
  return parts.join(' ');
}

export function TokenPathGraph({
  responseTokens,
  counterfactualTokens,
  responseTokenCount,
  counterfactualTokenCount,
  title = 'Token path graph',
}: {
  responseTokens: string[];
  counterfactualTokens: string[];
  responseTokenCount?: number;
  counterfactualTokenCount?: number;
  title?: string;
}) {
  const [selectedPos, setSelectedPos] = useState<number | null>(null);
  const [showMatched, setShowMatched] = useState(true);
  const [altDensity, setAltDensity] = useState(48);
  const [tip, setTip] = useState<TipState | null>(null);
  const [plotSize, setPlotSize] = useState({ w: 800, h: PLOT_H });
  const wrapRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLDivElement>(null);
  const longPressRef = useRef<number | null>(null);
  const suppressClickRef = useRef(false);

  const clearLongPress = useCallback(() => {
    if (longPressRef.current != null) {
      window.clearTimeout(longPressRef.current);
      longPressRef.current = null;
    }
  }, []);

  useEffect(() => () => clearLongPress(), [clearLongPress]);

  // Match viewBox to CSS pixels so circles/text are never stretched.
  useEffect(() => {
    const el = canvasRef.current;
    if (!el) return;
    const apply = (w: number, h: number) => {
      const nextW = Math.max(280, Math.round(w));
      const nextH = Math.max(140, Math.round(h));
      setPlotSize((prev) => (prev.w === nextW && prev.h === nextH ? prev : { w: nextW, h: nextH }));
    };
    apply(el.clientWidth, el.clientHeight);
    const ro = new ResizeObserver((entries) => {
      const cr = entries[0]?.contentRect;
      if (cr) apply(cr.width, cr.height);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const hasRealTokens = responseTokens.length > 0 || counterfactualTokens.length > 0;

  const { matched, alts: rawAlts } = useMemo(() => {
    if (hasRealTokens) {
      return buildPathsFromTokens(responseTokens, counterfactualTokens, altDensity);
    }
    return buildSyntheticPaths(
      responseTokenCount ?? 48,
      counterfactualTokenCount ?? 160,
      `counts-${responseTokenCount}-${counterfactualTokenCount}`,
      altDensity,
    );
  }, [
    hasRealTokens,
    responseTokens,
    counterfactualTokens,
    responseTokenCount,
    counterfactualTokenCount,
    altDensity,
  ]);

  const matchedWeights = useMemo(() => matched.points.map((p) => p.weight), [matched]);
  // Fit Y to the green matched path only — then remap alts into that band so they stay visible.
  const { minW, maxW } = useMemo(() => weightBand(matchedWeights), [matchedWeights]);
  const alts = useMemo(() => fitAltsIntoBand(rawAlts, minW, maxW), [rawAlts, minW, maxW]);
  const allWeights = useMemo(
    () => [...matchedWeights, ...alts.flatMap((a) => a.points.map((p) => p.weight))],
    [matchedWeights, alts],
  );
  const maxPos = Math.max(matched.points.length - 1, ...alts.map((a) => a.points.length - 1), 1);
  const altStrokeOpacity = Math.min(0.42, 0.14 + 10 / Math.max(altDensity, 1));

  const W = plotSize.w;
  const H = plotSize.h;
  const padL = 52;
  const padR = 16;
  const padT = 14;
  const padB = 28;
  const innerW = Math.max(40, W - padL - padR);
  const innerH = Math.max(40, H - padT - padB);

  const yTicks = useMemo(() => logTicks(minW, maxW, 4), [minW, maxW]);
  const xTicks = Array.from({ length: 6 }, (_, i) => Math.round((i / 5) * maxPos));

  const selected = selectedPos != null ? matched.points[selectedPos] : tip?.point ?? null;
  const topPaths = useMemo(
    () => [matched, ...alts].sort((a, b) => b.score - a.score).slice(0, 3),
    [matched, alts],
  );

  const bins = histBins(allWeights, minW, maxW);
  const maxBin = Math.max(...bins, 1);
  const avgW = allWeights.length ? allWeights.reduce((a, b) => a + b, 0) / allWeights.length : 0;
  const maxPathW = Math.max(...[matched.score, ...alts.map((a) => a.score)], 0);
  const matchedCount = 1 + alts.filter((a) => a.score >= matched.score * 0.85).length;

  const showTip = useCallback(
    (point: PathPoint, clientX: number, clientY: number, pinned: boolean) => {
      const canvas = canvasRef.current;
      const place: 'above' | 'below' =
        canvas && clientY - canvas.getBoundingClientRect().top < 72 ? 'below' : 'above';
      setTip({ point, clientX, clientY, pinned, place });
      if (pinned) setSelectedPos(point.position);
    },
    [],
  );

  const hideTip = useCallback(() => {
    setTip((prev) => (prev?.pinned ? prev : null));
  }, []);

  const onNodePointerEnter = (p: PathPoint, e: React.MouseEvent | React.PointerEvent) => {
    if ('pointerType' in e && e.pointerType === 'touch') return;
    showTip(p, e.clientX, e.clientY, false);
  };

  const onNodeClick = (p: PathPoint, e: React.MouseEvent) => {
    if (suppressClickRef.current) {
      suppressClickRef.current = false;
      return;
    }
    e.stopPropagation();
    if (selectedPos === p.position && tip?.pinned) {
      setSelectedPos(null);
      setTip(null);
      return;
    }
    showTip(p, e.clientX, e.clientY, true);
  };

  const onNodeTouchStart = (p: PathPoint, e: React.TouchEvent) => {
    const touch = e.touches[0];
    if (!touch) return;
    clearLongPress();
    longPressRef.current = window.setTimeout(() => {
      suppressClickRef.current = true;
      showTip(p, touch.clientX, touch.clientY, true);
      if (typeof navigator !== 'undefined' && 'vibrate' in navigator) {
        try {
          navigator.vibrate?.(12);
        } catch {
          /* ignore */
        }
      }
    }, LONG_PRESS_MS);
  };

  const onNodeTouchEnd = () => {
    clearLongPress();
  };

  const tipStyle =
    tip && canvasRef.current
      ? (() => {
          const rect = canvasRef.current.getBoundingClientRect();
          const left = Math.min(Math.max(8, tip.clientX - rect.left - 20), Math.max(8, rect.width - 220));
          const top =
            tip.place === 'below'
              ? Math.min(rect.height - 12, tip.clientY - rect.top + 14)
              : Math.max(8, tip.clientY - rect.top - 10);
          return { left, top };
        })()
      : undefined;

  return (
    <div
      className="tv-graph"
      ref={wrapRef}
      onClick={() => {
        if (tip && !tip.pinned) setTip(null);
      }}
    >
      <div className="tv-graph-toolbar">
        <div className="tv-graph-legend">
          <span className="tv-legend-item tv-legend-item--matched">Matched (with ax)</span>
          <span className="tv-legend-item tv-legend-item--alt">Alt paths</span>
        </div>
        <label className="tv-graph-ctrl">
          <HoverTip
            tip={
              <>
                <strong>Alt paths</strong>
                <span>
                  How many grey alternative (counterfactual) paths to draw behind the green matched
                  path. Higher = denser spaghetti; does not change measured savings.
                </span>
              </>
            }
          >
            <span>Alt</span>
          </HoverTip>
          <input
            type="range"
            min={8}
            max={64}
            step={8}
            value={altDensity}
            onChange={(e) => setAltDensity(Number(e.target.value))}
            aria-label="Alternate path count"
          />
          <span className="tv-graph-ctrl-val">{altDensity}</span>
        </label>
        <label className="tv-graph-toggle">
          <input type="checkbox" checked={showMatched} onChange={(e) => setShowMatched(e.target.checked)} />
          <HoverTip
            tip={
              <>
                <strong>Matched path</strong>
                <span>
                  Toggle the green “with ax” path on or off. Alt paths stay visible so you can compare
                  the background alone.
                </span>
              </>
            }
          >
            <span>Matched</span>
          </HoverTip>
        </label>
        <span className="tv-graph-hint">X = token # in response (not clock time)</span>
      </div>

      <div className="tv-graph-canvas-wrap" ref={canvasRef}>
        <svg
          className="tv-graph-svg"
          width={W}
          height={H}
          viewBox={`0 0 ${W} ${H}`}
          preserveAspectRatio="xMidYMid meet"
          role="img"
          aria-label={title}
        >
          <defs>
            <linearGradient id="tvPlotFade" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#12181f" />
              <stop offset="100%" stopColor="#0a0e12" />
            </linearGradient>
          </defs>
          <rect x={0} y={0} width={W} height={H} fill="url(#tvPlotFade)" rx={6} />
          <g transform={`translate(${padL},${padT})`}>
            {yTicks.map((t, i) => {
              const y = logY(t, minW, maxW, innerH);
              return (
                <g key={`yt-${i}`}>
                  <line x1={0} y1={y} x2={innerW} y2={y} className="tv-graph-grid" />
                  <text x={-8} y={y + 3.5} textAnchor="end" className="tv-graph-axis-label">
                    {formatWeightTick(t)}
                  </text>
                </g>
              );
            })}
            {xTicks.map((t) => {
              const x = (t / Math.max(maxPos, 1)) * innerW;
              return (
                <g key={`x-${t}`}>
                  <line x1={x} y1={0} x2={x} y2={innerH} className="tv-graph-grid tv-graph-grid--v" />
                  <text x={x} y={innerH + 16} textAnchor="middle" className="tv-graph-axis-label">
                    {t}
                  </text>
                </g>
              );
            })}

            {alts.map((path) => (
              <polyline
                key={path.id}
                fill="none"
                points={polyline(path.points, innerW, innerH, minW, maxW, maxPos)}
                className="tv-graph-alt"
                style={{ strokeOpacity: altStrokeOpacity }}
              />
            ))}

            {showMatched && (
              <>
                <polyline
                  fill="none"
                  points={polyline(matched.points, innerW, innerH, minW, maxW, maxPos)}
                  className="tv-graph-matched-glow"
                />
                <polyline
                  fill="none"
                  points={polyline(matched.points, innerW, innerH, minW, maxW, maxPos)}
                  className="tv-graph-matched"
                />
                {matched.points.map((p) => {
                  const x = (p.position / Math.max(maxPos, 1)) * innerW;
                  const y = logY(p.weight, minW, maxW, innerH);
                  const active = selectedPos === p.position || tip?.point.position === p.position;
                  return (
                    <circle
                      key={p.position}
                      cx={x}
                      cy={y}
                      r={active ? 5 : 3.2}
                      className={active ? 'tv-graph-node tv-graph-node--active' : 'tv-graph-node'}
                      onMouseEnter={(e) => onNodePointerEnter(p, e)}
                      onMouseMove={(e) => onNodePointerEnter(p, e)}
                      onMouseLeave={hideTip}
                      onClick={(e) => onNodeClick(p, e)}
                      onTouchStart={(e) => onNodeTouchStart(p, e)}
                      onTouchEnd={onNodeTouchEnd}
                      onTouchCancel={onNodeTouchEnd}
                    />
                  );
                })}
              </>
            )}
          </g>
        </svg>

        {tip && tipStyle && (
          <div
            className={`tv-graph-tooltip tv-graph-tooltip--${tip.place}${tip.pinned ? ' tv-graph-tooltip--pinned' : ''}`}
            style={tipStyle}
            role="tooltip"
          >
            {!hasRealTokens && (
              <div className="tv-graph-tooltip-note">Estimated path (period totals)</div>
            )}
            <div className="tv-graph-tooltip-row">
              <span>Position</span>
              <strong>{tip.point.position}</strong>
            </div>
            <div className="tv-graph-tooltip-row">
              <span>Token</span>
              <strong className="mono">
                {formatTokenLabel(tip.point.token, tip.point.position)}
              </strong>
            </div>
            <div className="tv-graph-tooltip-row">
              <span>Weight</span>
              <strong className="tv-token-weight">{tip.point.weight.toFixed(3)}</strong>
            </div>
            <div className="tv-graph-tooltip-preview">
              {formatPreview(matched.points, tip.point.position)}
            </div>
            {tip.pinned && (
              <button
                type="button"
                className="tv-graph-tooltip-close"
                onClick={(e) => {
                  e.stopPropagation();
                  setTip(null);
                  setSelectedPos(null);
                }}
              >
                Close
              </button>
            )}
          </div>
        )}
      </div>

      <div className="tv-graph-footer">
        <div className="tv-graph-kpis">
          <HoverTip
            tip={
              <>
                <strong>Paths</strong>
                <span>Matched path plus {alts.length} alternate counterfactual paths drawn as grey lines.</span>
              </>
            }
          >
            <div className="tv-kpi">
              <span className="tv-kpi-label">Paths</span>
              <span className="tv-kpi-value">{1 + alts.length}</span>
            </div>
          </HoverTip>
          <HoverTip
            tip={
              <>
                <strong>Matched</strong>
                <span>
                  Paths scoring within 15% of the matched path ({matched.score.toFixed(2)}). Higher means the
                  ax path is less unique vs alternatives.
                </span>
              </>
            }
          >
            <div className="tv-kpi tv-kpi--accent">
              <span className="tv-kpi-label">Matched</span>
              <span className="tv-kpi-value">{matchedCount}</span>
            </div>
          </HoverTip>
          <HoverTip
            tip={
              <>
                <strong>Avg weight</strong>
                <span>Mean token weight across matched + alt points. Relative rarity / length signal — not model logits.</span>
              </>
            }
          >
            <div className="tv-kpi">
              <span className="tv-kpi-label">Avg</span>
              <span className="tv-kpi-value">{avgW.toFixed(2)}</span>
            </div>
          </HoverTip>
          <HoverTip
            tip={
              <>
                <strong>Max path score</strong>
                <span>Highest geometric-mean path score among matched and alt paths ({maxPathW.toFixed(3)}).</span>
              </>
            }
          >
            <div className="tv-kpi tv-kpi--accent">
              <span className="tv-kpi-label">Max</span>
              <span className="tv-kpi-value">{maxPathW.toFixed(2)}</span>
            </div>
          </HoverTip>
        </div>

        <div className="tv-hist" role="img" aria-label="Weight distribution histogram">
          {bins.map((c, i) => {
            const lo = Math.log10(Math.max(minW, 1e-9));
            const hi = Math.log10(Math.max(maxW, minW * 1.01));
            const a = 10 ** (lo + (i / bins.length) * (hi - lo));
            const b = 10 ** (lo + ((i + 1) / bins.length) * (hi - lo));
            return (
              <HoverTip
                key={i}
                prefer="above"
                tip={
                  <>
                    <strong>Weight bin</strong>
                    <span>
                      {formatWeightTick(a)} – {formatWeightTick(b)}
                    </span>
                    <span>
                      {c.toLocaleString()} point{c === 1 ? '' : 's'}
                    </span>
                  </>
                }
              >
                <div
                  className="tv-hist-bar"
                  style={{ height: `${Math.max(10, (c / maxBin) * 100)}%` }}
                />
              </HoverTip>
            );
          })}
        </div>

        <ol className="tv-top-paths">
          {topPaths.map((p, i) => (
            <li key={p.id} className={p.matched ? 'tv-top-path tv-top-path--matched' : 'tv-top-path'}>
              <HoverTip
                tip={
                  <>
                    <strong>
                      #{i + 1} {p.matched ? 'Matched path' : p.label}
                    </strong>
                    <span>Score {p.score.toFixed(3)} (geometric mean of token weights)</span>
                    <span>
                      {p.points.length} tokens
                      {p.matched ? ' · green path in the plot' : ' · grey alternative'}
                    </span>
                  </>
                }
              >
                <span className="tv-top-path-inner">
                  <span className="tv-top-rank">{i + 1}</span>
                  <span className="tv-top-label">
                    {p.matched
                      ? p.points
                          .slice(0, 8)
                          .map((x) => formatTokenLabel(x.token, x.position))
                          .join(' ')
                          .slice(0, 42) || p.label
                      : p.label}
                  </span>
                  <span className="tv-top-score">{p.score.toFixed(2)}</span>
                </span>
              </HoverTip>
            </li>
          ))}
        </ol>

        <HoverTip
          tip={
            selected ? (
              <>
                <strong>Selected token</strong>
                <span>
                  #{selected.position} · {formatTokenLabel(selected.token, selected.position)}
                </span>
                <span>Weight {selected.weight.toFixed(3)}</span>
              </>
            ) : (
              <>
                <strong>Selection</strong>
                <span>Hover or click a green node in the plot to inspect a token.</span>
              </>
            )
          }
        >
          <div className="tv-selected">
            {selected ? (
              <>
                <span className="tv-selected-pos">#{selected.position}</span>
                <span className="tv-selected-token mono">
                  {formatTokenLabel(selected.token, selected.position)}
                </span>
                <span className="tv-token-weight">{selected.weight.toFixed(3)}</span>
              </>
            ) : (
              <span className="tv-selected-hint">Hover a node for details</span>
            )}
          </div>
        </HoverTip>
      </div>
    </div>
  );
}
