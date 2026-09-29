import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import {
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  forceX,
  forceY,
  type Simulation,
  type SimulationLinkDatum,
  type SimulationNodeDatum,
} from 'd3-force';
import GraphSettingsPanel from './GraphSettingsPanel';
import { loadGraphSettings, saveGraphSettings, type GraphSettings } from '../lib/graphSettings';
import { filterGraph, groupColor, type FilterEdge } from '../lib/graphFilter';
import { buildNeighbors, highlightFor, selectionPulse } from '../lib/graphHover';
import { forceParams, seedCluster } from '../lib/graphForces';
import { labelAlpha, labelPx, placeLabels, type LabelBox } from '../lib/graphLabels';

export interface CanvasNode {
  key: string;
  label: string;
  kind: string;
  color: string;
  /** Drawn with an outline ring (for example global items). */
  ring?: boolean;
  degree: number;
}

export interface CanvasEdge {
  source: string;
  target: string;
}

interface Props {
  nodes: CanvasNode[];
  edges: CanvasEdge[];
  selectedKey: string | null;
  onSelect: (key: string | null) => void;
  settingsKey: string;
  legend?: ReactNode;
}

interface SimNode extends SimulationNodeDatum {
  i: number;
}

const WORLD_UNIT = 0.3;
const CLICK_SLOP = 4;

function themeAccent(): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim();
  return v || '#7c4dff';
}

function radius(degree: number, size: number): number {
  return Math.min(2.5 + Math.sqrt(degree) * 0.6, 12) * size;
}

export default function ForceGraphCanvas({ nodes, edges, selectedKey, onSelect, settingsKey, legend }: Props) {
  const [settings, setSettings] = useState<GraphSettings>(() => loadGraphSettings(undefined, settingsKey));
  const [settingsOpen, setSettingsOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const simRef = useRef<Simulation<SimNode, SimulationLinkDatum<SimNode>> | null>(null);
  const simNodesRef = useRef<SimNode[]>([]);
  const view = useRef({ x: 0, y: 0, k: 1 });
  const hoverRef = useRef<number | null>(null);
  const selectedRef = useRef<number | null>(null);
  const drawRef = useRef<() => void>(() => {});

  const index = useMemo(() => new Map(nodes.map((n, i) => [n.key, i])), [nodes]);
  const allEdges = useMemo<FilterEdge[]>(
    () =>
      edges.flatMap((e) => {
        const s = index.get(e.source);
        const t = index.get(e.target);
        return s == null || t == null ? [] : [{ source: s, target: t }];
      }),
    [edges, index],
  );
  const visible = useMemo(
    () =>
      filterGraph(
        nodes.map((n) => ({ name: n.label, kind: n.kind, file_path: '' })),
        allEdges,
        settings,
      ),
    [nodes, allEdges, settings],
  );
  const shownEdges = useMemo(() => visible.edges.map((i) => allEdges[i]), [visible, allEdges]);
  const neighbors = useMemo(() => buildNeighbors(nodes.length, shownEdges), [nodes.length, shownEdges]);

  selectedRef.current = selectedKey == null ? null : (index.get(selectedKey) ?? null);

  const updateSettings = (next: GraphSettings) => {
    setSettings(next);
    saveGraphSettings(next, undefined, settingsKey);
  };

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext('2d');
    if (!canvas || !ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
      canvas.width = w * dpr;
      canvas.height = h * dpr;
    }
    const { x: vx, y: vy, k } = view.current;
    const accent = themeAccent();
    const sim = simNodesRef.current;
    const hl = highlightFor(hoverRef.current, neighbors);
    const dim = hoverRef.current != null;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.setTransform(dpr * k, 0, 0, dpr * k, dpr * vx, dpr * vy);

    const style = getComputedStyle(document.documentElement);
    const lineColor = style.getPropertyValue('--text-dim').trim() || '#888';
    shownEdges.forEach((e, ei) => {
      const a = sim[e.source];
      const b = sim[e.target];
      if (!a || !b) return;
      const on = hl.edges.has(ei);
      ctx.globalAlpha = dim && !on ? 0.12 : on ? 1 : 0.45;
      ctx.strokeStyle = on ? accent : lineColor;
      ctx.lineWidth = (settings.linkThickness * 0.7) / k;
      ctx.beginPath();
      ctx.moveTo(a.x!, a.y!);
      ctx.lineTo(b.x!, b.y!);
      ctx.stroke();
      if (settings.arrows) {
        const r = radius(nodes[e.target].degree, settings.nodeSize);
        const ang = Math.atan2(b.y! - a.y!, b.x! - a.x!);
        const tx = b.x! - Math.cos(ang) * r;
        const ty = b.y! - Math.sin(ang) * r;
        const s = 5 / k;
        ctx.fillStyle = ctx.strokeStyle;
        ctx.beginPath();
        ctx.moveTo(tx, ty);
        ctx.lineTo(tx - s * Math.cos(ang - 0.4), ty - s * Math.sin(ang - 0.4));
        ctx.lineTo(tx - s * Math.cos(ang + 0.4), ty - s * Math.sin(ang + 0.4));
        ctx.fill();
      }
    });

    const labels: LabelBox[] = [];
    const labelIdx: number[] = [];
    const px = labelPx(k);
    const fade = labelAlpha(k, settings.textFadeThreshold);
    ctx.font = `${px}px system-ui, sans-serif`;
    nodes.forEach((n, i) => {
      if (!visible.nodes.has(i)) return;
      const p = sim[i];
      if (!p) return;
      const r = radius(n.degree, settings.nodeSize);
      const on = hl.nodes.has(i);
      ctx.globalAlpha = dim && !on ? 0.25 : 1;
      ctx.fillStyle = i === hoverRef.current ? accent : (groupColor({ name: n.label, kind: n.kind, file_path: '' }, settings.groups) ?? n.color);
      ctx.beginPath();
      ctx.arc(p.x!, p.y!, r, 0, Math.PI * 2);
      ctx.fill();
      if (n.ring) {
        ctx.strokeStyle = accent;
        ctx.lineWidth = 1.5 / k;
        ctx.beginPath();
        ctx.arc(p.x!, p.y!, r + 2 / k, 0, Math.PI * 2);
        ctx.stroke();
      }
      const force = on || i === selectedRef.current;
      if (fade > 0 || force) {
        const sx = p.x! * k + vx;
        const sy = (p.y! + r) * k + vy + 4;
        const tw = ctx.measureText(n.label).width / k;
        labels.push({ x: sx - (tw * k) / 2, y: sy, w: tw * k, h: px, priority: force ? 1e6 : n.degree });
        labelIdx.push(i);
      }
    });

    const sel = selectedRef.current;
    if (sel != null && sim[sel]) {
      const p = sim[sel];
      const r = radius(nodes[sel].degree, settings.nodeSize);
      const { grow, alpha } = selectionPulse(performance.now());
      ctx.globalAlpha = alpha;
      ctx.strokeStyle = accent;
      ctx.lineWidth = 2 / k;
      ctx.beginPath();
      ctx.arc(p.x!, p.y!, r + (4 + grow * 14) / k, 0, Math.PI * 2);
      ctx.stroke();
    }

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.font = `${px}px system-ui, sans-serif`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'top';
    ctx.fillStyle = style.getPropertyValue('--text').trim() || '#ddd';
    for (const li of placeLabels(labels)) {
      const i = labelIdx[li];
      const forced = hl.nodes.has(i) || i === sel;
      ctx.globalAlpha = forced ? 1 : dim ? fade * 0.3 : fade;
      const b = labels[li];
      ctx.fillText(nodes[i].label, b.x + b.w / 2, b.y);
    }
    ctx.globalAlpha = 1;
  }, [nodes, shownEdges, neighbors, visible, settings]);
  drawRef.current = draw;

  const restart = useCallback(
    (alpha: number) => {
      const canvas = canvasRef.current;
      const w = canvas?.clientWidth ?? 800;
      const h = canvas?.clientHeight ?? 600;
      const prev = simNodesRef.current;
      const seeds = seedCluster(nodes.length, 0, 0, 4 * Math.sqrt(Math.max(1, nodes.length)));
      const sim: SimNode[] = nodes.map((_, i) => ({ i, x: prev[i]?.x ?? seeds[i].x, y: prev[i]?.y ?? seeds[i].y }));
      simNodesRef.current = sim;
      if (prev.length === 0) view.current = { x: w / 2, y: h / 2, k: 1 };
      const f = forceParams(settings);
      const degree = new Array(nodes.length).fill(0);
      for (const e of shownEdges) {
        degree[e.source]++;
        degree[e.target]++;
      }
      const shown = sim.filter((n) => visible.nodes.has(n.i));
      const links = shownEdges.map((e) => ({ source: sim[e.source], target: sim[e.target] }));
      simRef.current?.stop();
      simRef.current = forceSimulation(shown)
        .force('charge', forceManyBody<SimNode>().strength(f.charge).distanceMax(900))
        .force(
          'link',
          forceLink<SimNode, SimulationLinkDatum<SimNode>>(links)
            .distance(f.linkDistance * WORLD_UNIT)
            .strength((l) => f.linkStrength / Math.max(1, Math.min(degree[(l.source as SimNode).i], degree[(l.target as SimNode).i]))),
        )
        .force('x', forceX(0).strength(f.center))
        .force('y', forceY(0).strength(f.center))
        .force('collide', forceCollide<SimNode>((n) => radius(nodes[n.i].degree, settings.nodeSize) + 14).strength(0.9))
        .alpha(alpha)
        .on('tick', () => drawRef.current());
    },
    [nodes, shownEdges, visible, settings],
  );

  useEffect(() => {
    restart(0.9);
  }, [restart]);

  useEffect(() => () => void simRef.current?.stop(), []);

  useEffect(() => {
    if (selectedKey == null) return;
    let raf = 0;
    const loop = () => {
      drawRef.current();
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [selectedKey]);

  useEffect(() => {
    if (!settingsOpen) return;
    const onDown = (e: PointerEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest('.graph-settings, .graph-settings-toggle')) return;
      setSettingsOpen(false);
    };
    document.addEventListener('pointerdown', onDown, true);
    return () => document.removeEventListener('pointerdown', onDown, true);
  }, [settingsOpen]);

  const hitTest = (clientX: number, clientY: number): number | null => {
    const rect = canvasRef.current!.getBoundingClientRect();
    const { x: vx, y: vy, k } = view.current;
    const wx = (clientX - rect.left - vx) / k;
    const wy = (clientY - rect.top - vy) / k;
    let best: number | null = null;
    let bestD = Infinity;
    for (const n of simNodesRef.current) {
      if (!visible.nodes.has(n.i)) continue;
      const d = Math.hypot(n.x! - wx, n.y! - wy);
      const r = radius(nodes[n.i].degree, settings.nodeSize) + 4 / k;
      if (d <= r && d < bestD) {
        best = n.i;
        bestD = d;
      }
    }
    return best;
  };

  const setHover = (i: number | null) => {
    if (hoverRef.current === i) return;
    hoverRef.current = i;
    if (wrapRef.current) wrapRef.current.dataset.hover = i == null ? '' : nodes[i].key;
    draw();
  };

  const drag = useRef<{ node: number | null; sx: number; sy: number; vx: number; vy: number; moved: boolean } | null>(null);

  const onPointerDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    const node = hitTest(e.clientX, e.clientY);
    drag.current = { node, sx: e.clientX, sy: e.clientY, vx: view.current.x, vy: view.current.y, moved: false };
  };

  const onPointerMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const d = drag.current;
    if (!d) {
      setHover(hitTest(e.clientX, e.clientY));
      return;
    }
    const dx = e.clientX - d.sx;
    const dy = e.clientY - d.sy;
    if (Math.hypot(dx, dy) > CLICK_SLOP) d.moved = true;
    if (!d.moved) return;
    if (d.node != null) {
      const rect = canvasRef.current!.getBoundingClientRect();
      const n = simNodesRef.current[d.node];
      n.fx = (e.clientX - rect.left - view.current.x) / view.current.k;
      n.fy = (e.clientY - rect.top - view.current.y) / view.current.k;
      simRef.current?.alphaTarget(0.3).restart();
    } else {
      view.current.x = d.vx + dx;
      view.current.y = d.vy + dy;
      draw();
    }
  };

  const onPointerUp = () => {
    const d = drag.current;
    drag.current = null;
    if (!d) return;
    if (d.node != null && d.moved) {
      const n = simNodesRef.current[d.node];
      n.fx = null;
      n.fy = null;
      simRef.current?.alphaTarget(0);
      return;
    }
    if (!d.moved) onSelect(d.node == null ? null : nodes[d.node].key);
  };

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = canvas.getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      const v = view.current;
      const k = Math.min(8, Math.max(0.1, v.k * Math.exp(-e.deltaY * 0.0015)));
      v.x = mx - ((mx - v.x) * k) / v.k;
      v.y = my - ((my - v.y) * k) / v.k;
      v.k = k;
      drawRef.current();
    };
    canvas.addEventListener('wheel', onWheel, { passive: false });
    return () => canvas.removeEventListener('wheel', onWheel);
  }, []);

  useEffect(() => {
    const onResize = () => drawRef.current();
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  const centerOnSelection = () => {
    const i = selectedRef.current;
    const n = i == null ? null : simNodesRef.current[i];
    const canvas = canvasRef.current;
    if (!n || !canvas) return;
    view.current.x = canvas.clientWidth / 2 - n.x! * view.current.k;
    view.current.y = canvas.clientHeight / 2 - n.y! * view.current.k;
    draw();
  };

  return (
    <div className="graph-canvas-wrap" ref={wrapRef} data-hover="" data-selected={selectedKey ?? ''}>
      <canvas
        ref={canvasRef}
        className="graph-canvas"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={() => setHover(null)}
      />
      <div className="graph-canvas-tools" onPointerDown={(e) => e.stopPropagation()}>
        {selectedKey && (
          <button type="button" className="btn-secondary" onClick={centerOnSelection}>
            ◎ Show selection
          </button>
        )}
        <button
          type="button"
          className={`btn-secondary graph-settings-toggle${settingsOpen ? ' active' : ''}`}
          aria-label="Graph settings"
          aria-expanded={settingsOpen}
          onClick={() => setSettingsOpen((v) => !v)}
        >
          ⚙ Settings
        </button>
      </div>
      {legend && <div className="graph-legend graph-legend--bottom">{legend}</div>}
      {settingsOpen && (
        <GraphSettingsPanel
          settings={settings}
          onChange={updateSettings}
          onAnimate={() => {
            simNodesRef.current = [];
            restart(1);
          }}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </div>
  );
}
