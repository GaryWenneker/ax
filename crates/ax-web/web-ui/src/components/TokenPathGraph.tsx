import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { HoverTip } from './HoverTip';
import {
  buildPathsFromTokens,
  buildSyntheticPaths,
  fitAltsIntoBand,
  formatWeightTick,
  histBins,
  logTicks,
  logY,
  polyline,
  weightBand,
  type PathPoint,
} from '../lib/tokenPath';

export { buildPathsFromTokens, buildSyntheticPaths, type PathPoint, type TokenPath } from '../lib/tokenPath';

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

  const bins = useMemo(() => histBins(allWeights, minW, maxW), [allWeights, minW, maxW]);
  const maxBin = Math.max(...bins, 1);
  const avgW = allWeights.length ? allWeights.reduce((a, b) => a + b, 0) / allWeights.length : 0;
  const maxPathW = Math.max(...[matched.score, ...alts.map((a) => a.score)], 0);
  const matchedCount = 1 + alts.filter((a) => a.score >= matched.score * 0.85).length;

  // Hover and selection only change the tooltip and node radius; the lines, node
  // coordinates and histogram are rebuilt only when the data or plot size change.
  const altLines = useMemo(
    () => alts.map((path) => ({ id: path.id, points: polyline(path.points, innerW, innerH, minW, maxW, maxPos) })),
    [alts, innerW, innerH, minW, maxW, maxPos],
  );
  const matchedLine = useMemo(
    () => polyline(matched.points, innerW, innerH, minW, maxW, maxPos),
    [matched, innerW, innerH, minW, maxW, maxPos],
  );
  const nodes = useMemo(
    () =>
      matched.points.map((p) => ({
        point: p,
        x: (p.position / Math.max(maxPos, 1)) * innerW,
        y: logY(p.weight, minW, maxW, innerH),
      })),
    [matched, innerW, innerH, minW, maxW, maxPos],
  );

  // A tooltip or selection belongs to one call's tokens; drop it when the tokens change.
  useEffect(() => {
    setTip(null);
    setSelectedPos(null);
  }, [matched]);

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

            {altLines.map((path) => (
              <polyline
                key={path.id}
                fill="none"
                points={path.points}
                className="tv-graph-alt"
                style={{ strokeOpacity: altStrokeOpacity }}
              />
            ))}

            {showMatched && (
              <>
                <polyline
                  fill="none"
                  points={matchedLine}
                  className="tv-graph-matched-glow"
                />
                <polyline
                  fill="none"
                  points={matchedLine}
                  className="tv-graph-matched"
                />
                {nodes.map(({ point: p, x, y }) => {
                  const active = selectedPos === p.position || tip?.point.position === p.position;
                  return (
                    <circle
                      key={p.position}
                      cx={x}
                      cy={y}
                      r={active ? 5 : 3.2}
                      className={active ? 'tv-graph-node tv-graph-node--active' : 'tv-graph-node'}
                      onMouseEnter={(e) => onNodePointerEnter(p, e)}
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
