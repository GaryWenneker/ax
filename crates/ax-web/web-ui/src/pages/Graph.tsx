import { useEffect, useMemo, useRef, useState } from 'react';
import { streamGraph, type GraphNode, type GraphEdge, type GraphStreamMeta } from '../api';
import NodeDetailPanel from '../components/NodeDetail';
import { Spinner } from '../components/ui/Spinner';

interface SimNode extends GraphNode {
  x: number;
  y: number;
  vx: number;
  vy: number;
}

interface SimEdge {
  source: number;
  target: number;
  kind: string;
  confidence?: string;
}

// Distinct, high-contrast palette; community id is mapped modulo length.
const COMMUNITY_COLORS = [
  '#4ec9b0', '#569cd6', '#c586c0', '#dcdcaa', '#ce9178', '#9cdcfe',
  '#d7ba7d', '#4fc1ff', '#b5cea8', '#f48771', '#c8c8c8', '#e2c08d',
  '#6a9955', '#d16969', '#808080', '#7ca4cb',
];

function colorFor(communityId: number): string {
  if (communityId < 0) return '#666';
  return COMMUNITY_COLORS[communityId % COMMUNITY_COLORS.length];
}

function dashFor(confidence?: string): number[] {
  switch (confidence) {
    case 'inferred':
      return [4, 3];
    case 'ambiguous':
      return [1, 3];
    default:
      return []; // extracted → solid
  }
}

const DEFAULT_LIMIT = 600;
const MAX_ITERATIONS = 600;

export default function GraphPage() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const simNodesRef = useRef<SimNode[]>([]);
  const simEdgesRef = useRef<SimEdge[]>([]);
  const idIndexRef = useRef<Map<string, number>>(new Map());
  const rafRef = useRef<number | null>(null);
  const runningRef = useRef(false);
  const iterationsRef = useRef(0);
  const settledRef = useRef(false);
  const abortRef = useRef<null | (() => void)>(null);
  const transformRef = useRef({ scale: 1, offsetX: 0, offsetY: 0 });
  const draggingRef = useRef<{ node: SimNode | null; panning: boolean; lastX: number; lastY: number }>({
    node: null,
    panning: false,
    lastX: 0,
    lastY: 0,
  });
  const hoverRef = useRef<SimNode | null>(null);
  // While true, each simulation frame recenters + zooms the view to fit the
  // whole graph. Any manual pan/zoom/drag turns it off so we don't fight the user.
  const autoFitRef = useRef(true);
  // Lowercased set of node ids matching the current search, or null when the
  // search box is empty (no filtering / everything at full opacity).
  const matchRef = useRef<Set<string> | null>(null);

  const [meta, setMeta] = useState<GraphStreamMeta | null>(null);
  const [loadedNodes, setLoadedNodes] = useState(0);
  const [edgeCount, setEdgeCount] = useState(0);
  const [legendNodes, setLegendNodes] = useState<GraphNode[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadDone, setLoadDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [limit, setLimit] = useState(DEFAULT_LIMIT);
  const [search, setSearch] = useState('');
  const [kindFilter, setKindFilter] = useState('');
  const [matchCount, setMatchCount] = useState<number | null>(null);

  function resetGraphState() {
    if (rafRef.current != null) cancelAnimationFrame(rafRef.current);
    rafRef.current = null;
    runningRef.current = false;
    iterationsRef.current = 0;
    settledRef.current = false;
    simNodesRef.current = [];
    simEdgesRef.current = [];
    idIndexRef.current = new Map();
    matchRef.current = null;
    transformRef.current = { scale: 1, offsetX: 0, offsetY: 0 };
    autoFitRef.current = true;
    hoverRef.current = null;
  }

  function addNodes(batch: GraphNode[]) {
    const wrap = wrapRef.current;
    const w = wrap?.clientWidth ?? 800;
    const h = wrap?.clientHeight ?? 600;
    const nodes = simNodesRef.current;
    const idIndex = idIndexRef.current;
    const radius = Math.min(w, h) * 0.35;
    for (const n of batch) {
      if (idIndex.has(n.id)) continue;
      // Seed on a jittered ring around the center; the force layout settles it.
      const angle = Math.random() * Math.PI * 2;
      const r = radius * (0.4 + Math.random() * 0.6);
      nodes.push({ ...n, x: w / 2 + Math.cos(angle) * r, y: h / 2 + Math.sin(angle) * r, vx: 0, vy: 0 });
      idIndex.set(n.id, nodes.length - 1);
    }
  }

  function addEdges(batch: GraphEdge[]) {
    const idIndex = idIndexRef.current;
    const edges = simEdgesRef.current;
    for (const e of batch) {
      const s = idIndex.get(e.source);
      const t = idIndex.get(e.target);
      if (s != null && t != null && s !== t) {
        edges.push({ source: s, target: t, kind: e.kind, confidence: e.confidence });
      }
    }
  }

  function load(recompute = false) {
    if (abortRef.current) abortRef.current();
    resetGraphState();
    setMeta(null);
    setLegendNodes([]);
    setLoadedNodes(0);
    setEdgeCount(0);
    setLoadDone(false);
    setMatchCount(null);
    setLoading(true);
    setError(null);

    abortRef.current = streamGraph(
      { limit, recompute },
      {
        onMeta: (m) => setMeta(m),
        onNodes: (batch) => {
          addNodes(batch);
          setLoadedNodes(simNodesRef.current.length);
          setLegendNodes((prev) => prev.concat(batch));
          ensureSimulation();
        },
        onEdges: (batch) => {
          addEdges(batch);
          setEdgeCount(simEdgesRef.current.length);
        },
        onDone: () => {
          setLoading(false);
          setLoadDone(true);
          // Ensure at least one settle pass happens for tiny graphs that
          // finished before the loop noticed there was anything to do.
          ensureSimulation();
        },
      },
    );
  }

  useEffect(() => {
    load();
    return () => {
      if (abortRef.current) abortRef.current();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [limit]);

  const communityLegend = useMemo(() => {
    const seen = new Map<number, string>();
    for (const n of legendNodes) {
      if (n.community_id >= 0 && !seen.has(n.community_id)) {
        seen.set(n.community_id, n.community_label || `#${n.community_id}`);
      }
    }
    return Array.from(seen.entries())
      .slice(0, 16)
      .map(([id, label]) => ({ id, label, color: colorFor(id) }));
  }, [legendNodes]);

  const kindOptions = useMemo(() => {
    return Array.from(new Set(legendNodes.map((n) => n.kind))).sort();
  }, [legendNodes]);

  // Center on the dense hub core (high-degree nodes) and zoom in so the
  // important cluster fills the viewport instead of the scattered periphery.
  function fitView(final = false) {
    const canvas = canvasRef.current;
    const nodes = simNodesRef.current;
    if (!canvas || nodes.length === 0) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.width / dpr;
    const h = canvas.height / dpr;
    if (w <= 0 || h <= 0) return;

    const sorted = [...nodes].sort((a, b) => b.degree - a.degree);
    const coreCount = Math.max(12, Math.ceil(nodes.length * 0.35));
    const core = sorted.slice(0, coreCount);

    let sumW = 0;
    let sx = 0;
    let sy = 0;
    for (const n of core) {
      const weight = n.degree + 1;
      sumW += weight;
      sx += n.x * weight;
      sy += n.y * weight;
    }
    const gcx = sx / sumW;
    const gcy = sy / sumW;

    const dists = core.map((n) => Math.hypot(n.x - gcx, n.y - gcy)).sort((a, b) => a - b);
    const pct = final ? 0.65 : 0.75;
    const idx = Math.min(dists.length - 1, Math.floor(dists.length * pct));
    const radius = Math.max(1, dists[idx]);

    const pad = 48;
    let scale = Math.min((w - pad * 2) / (radius * 2), (h - pad * 2) / (radius * 2));
    scale *= final ? 2.0 : 1.5;
    scale = Math.max(0.1, Math.min(scale, 10));

    transformRef.current = {
      scale,
      offsetX: w / 2 - gcx * scale,
      offsetY: h / 2 - gcy * scale,
    };
  }

  // Recenter the viewport on a single node without changing zoom too much —
  // used when a search narrows down to one strong match.
  function centerOnNode(node: SimNode) {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.width / dpr;
    const h = canvas.height / dpr;
    const scale = Math.max(1.2, Math.min(transformRef.current.scale, 3));
    transformRef.current = { scale, offsetX: w / 2 - node.x * scale, offsetY: h / 2 - node.y * scale };
  }

  function applySearch(term: string, kind: string) {
    const q = term.trim().toLowerCase();
    const nodes = simNodesRef.current;
    if (!q && !kind) {
      matchRef.current = null;
      setMatchCount(null);
      requestDraw();
      return;
    }
    const matches = nodes.filter((n) => {
      const nameHit = !q || n.name.toLowerCase().includes(q) || n.id.toLowerCase().includes(q);
      const kindHit = !kind || n.kind === kind;
      return nameHit && kindHit;
    });
    matchRef.current = new Set(matches.map((n) => n.id));
    setMatchCount(matches.length);
    if (matches.length === 1) {
      setSelected(matches[0].id);
      centerOnNode(matches[0]);
    }
    requestDraw();
  }

  // Start (or keep) the physics loop. Adding nodes mid-flight resets the
  // settle state so the layout keeps relaxing as the stream fills in.
  function ensureSimulation() {
    settledRef.current = false;
    if (runningRef.current) return;
    runningRef.current = true;
    startSimulation();
  }

  function startSimulation() {
    if (rafRef.current != null) cancelAnimationFrame(rafRef.current);
    const canvas = canvasRef.current;
    const wrap = wrapRef.current;
    if (!canvas || !wrap) {
      runningRef.current = false;
      return;
    }

    const step = () => {
      const nodes = simNodesRef.current;
      const edges = simEdgesRef.current;
      const w = canvas.width / (window.devicePixelRatio || 1);
      const h = canvas.height / (window.devicePixelRatio || 1);
      const cx = w / 2;
      const cy = h / 2;

      if (iterationsRef.current < MAX_ITERATIONS && nodes.length > 0) {
        const k = Math.sqrt((w * h) / Math.max(1, nodes.length));
        const cell = Math.max(1, k);

        // Uniform spatial grid: only repel against nodes in the same and
        // neighboring cells. This turns the O(n^2) repulsion into ~O(n),
        // which is what keeps large graphs from freezing while they load.
        const grid = new Map<number, number[]>();
        const cols = Math.max(1, Math.ceil(w / cell) + 4);
        const cellKey = (gx: number, gy: number) => (gy + 2) * cols + (gx + 2);
        for (let i = 0; i < nodes.length; i++) {
          const gx = Math.floor(nodes[i].x / cell);
          const gy = Math.floor(nodes[i].y / cell);
          const key = cellKey(gx, gy);
          let arr = grid.get(key);
          if (!arr) {
            arr = [];
            grid.set(key, arr);
          }
          arr.push(i);
        }

        for (let i = 0; i < nodes.length; i++) {
          const a = nodes[i];
          const gx = Math.floor(a.x / cell);
          const gy = Math.floor(a.y / cell);
          for (let ox = -1; ox <= 1; ox++) {
            for (let oy = -1; oy <= 1; oy++) {
              const arr = grid.get(cellKey(gx + ox, gy + oy));
              if (!arr) continue;
              for (const j of arr) {
                if (j <= i) continue; // count each unordered pair once
                const b = nodes[j];
                let dx = a.x - b.x;
                let dy = a.y - b.y;
                let dist2 = dx * dx + dy * dy;
                if (dist2 < 0.01) {
                  dx = Math.random() - 0.5;
                  dy = Math.random() - 0.5;
                  dist2 = 0.01;
                }
                const dist = Math.sqrt(dist2);
                const force = (k * k) / dist;
                const fx = (dx / dist) * force;
                const fy = (dy / dist) * force;
                a.vx += fx;
                a.vy += fy;
                b.vx -= fx;
                b.vy -= fy;
              }
            }
          }
        }

        // Attraction along edges (springs).
        for (const e of edges) {
          const a = nodes[e.source];
          const b = nodes[e.target];
          if (!a || !b) continue;
          const dx = a.x - b.x;
          const dy = a.y - b.y;
          const dist = Math.sqrt(dx * dx + dy * dy) || 0.01;
          const force = (dist * dist) / k;
          const fx = (dx / dist) * force;
          const fy = (dy / dist) * force;
          a.vx -= fx;
          a.vy -= fy;
          b.vx += fx;
          b.vy += fy;
        }

        // Gravity toward center + integrate with cooling.
        const cooling = 1 - iterationsRef.current / MAX_ITERATIONS;
        const maxDisp = 30 * cooling + 1;
        for (const n of nodes) {
          n.vx += (cx - n.x) * 0.002;
          n.vy += (cy - n.y) * 0.002;
          if (draggingRef.current.node === n) {
            n.vx = 0;
            n.vy = 0;
            continue;
          }
          const disp = Math.sqrt(n.vx * n.vx + n.vy * n.vy) || 0.01;
          const limited = Math.min(disp, maxDisp);
          n.x += (n.vx / disp) * limited;
          n.y += (n.vy / disp) * limited;
          n.vx *= 0.85;
          n.vy *= 0.85;
        }
        iterationsRef.current++;
        if (autoFitRef.current && iterationsRef.current > MAX_ITERATIONS - 80) {
          fitView(iterationsRef.current >= MAX_ITERATIONS);
        }
        draw();
        rafRef.current = requestAnimationFrame(step);
      } else {
        // Layout settled — one final aggressive zoom, then stop the loop so we
        // don't burn CPU redrawing a static graph. Interactions redraw on demand.
        if (autoFitRef.current) {
          fitView(true);
          autoFitRef.current = false;
        }
        settledRef.current = true;
        runningRef.current = false;
        rafRef.current = null;
        draw();
      }
    };
    rafRef.current = requestAnimationFrame(step);
  }

  // Schedule a single redraw (used for hover/drag/pan/zoom once the layout is
  // settled and the animation loop has stopped).
  function requestDraw() {
    if (rafRef.current != null) return;
    rafRef.current = requestAnimationFrame(() => {
      rafRef.current = null;
      draw();
    });
  }

  function draw() {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.width / dpr;
    const h = canvas.height / dpr;
    const { scale, offsetX, offsetY } = transformRef.current;

    ctx.save();
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.translate(offsetX, offsetY);
    ctx.scale(scale, scale);

    const nodes = simNodesRef.current;
    const edges = simEdgesRef.current;
    const match = matchRef.current;
    const isMatch = (n: SimNode) => match == null || match.has(n.id);

    // Edges. When a search is active, only edges touching a match stay lit.
    ctx.lineWidth = 0.6;
    for (const e of edges) {
      const a = nodes[e.source];
      const b = nodes[e.target];
      if (!a || !b) continue;
      ctx.beginPath();
      ctx.setLineDash(dashFor(e.confidence));
      const highlight =
        hoverRef.current && (nodes[e.source] === hoverRef.current || nodes[e.target] === hoverRef.current);
      const dimmed = match != null && !isMatch(a) && !isMatch(b);
      ctx.strokeStyle = dimmed
        ? 'rgba(140,140,160,0.04)'
        : highlight
          ? 'rgba(200,200,255,0.7)'
          : 'rgba(140,140,160,0.18)';
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.stroke();
    }
    ctx.setLineDash([]);

    // Nodes.
    for (const n of nodes) {
      const r = Math.min(3 + Math.sqrt(n.degree) * 1.6, 22);
      const isDoc = n.kind === 'doc';
      const dimmed = !isMatch(n);
      ctx.globalAlpha = dimmed ? 0.12 : 1;
      ctx.beginPath();
      ctx.fillStyle = isDoc ? '#e0b341' : colorFor(n.community_id);
      if (isDoc) {
        ctx.rect(n.x - r, n.y - r, r * 2, r * 2);
      } else {
        ctx.arc(n.x, n.y, r, 0, Math.PI * 2);
      }
      ctx.fill();
      if (!dimmed && (n.id === selected || n === hoverRef.current)) {
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#fff';
        ctx.stroke();
      }
      if (!dimmed && (r > 8 || n === hoverRef.current || n.id === selected || match != null)) {
        ctx.fillStyle = 'rgba(230,230,230,0.9)';
        ctx.font = '10px var(--font-mono, monospace)';
        ctx.fillText(n.name, n.x + r + 2, n.y + 3);
      }
    }
    ctx.globalAlpha = 1;
    ctx.restore();
  }

  // Canvas sizing to device pixels.
  useEffect(() => {
    const canvas = canvasRef.current;
    const wrap = wrapRef.current;
    if (!canvas || !wrap) return;
    const resize = () => {
      const dpr = window.devicePixelRatio || 1;
      canvas.width = wrap.clientWidth * dpr;
      canvas.height = wrap.clientHeight * dpr;
      canvas.style.width = `${wrap.clientWidth}px`;
      canvas.style.height = `${wrap.clientHeight}px`;
      if (autoFitRef.current) fitView();
      draw();
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(wrap);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Re-run the search when the term, kind filter, or completed dataset changes.
  useEffect(() => {
    applySearch(search, kindFilter);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [search, kindFilter, loadDone]);

  useEffect(() => {
    return () => {
      if (rafRef.current != null) cancelAnimationFrame(rafRef.current);
      if (abortRef.current) abortRef.current();
    };
  }, []);

  function toWorld(clientX: number, clientY: number): { x: number; y: number } {
    const canvas = canvasRef.current!;
    const rect = canvas.getBoundingClientRect();
    const { scale, offsetX, offsetY } = transformRef.current;
    const px = clientX - rect.left;
    const py = clientY - rect.top;
    return { x: (px - offsetX) / scale, y: (py - offsetY) / scale };
  }

  function nodeAt(clientX: number, clientY: number): SimNode | null {
    const { x, y } = toWorld(clientX, clientY);
    const nodes = simNodesRef.current;
    for (let i = nodes.length - 1; i >= 0; i--) {
      const n = nodes[i];
      const r = Math.min(3 + Math.sqrt(n.degree) * 1.6, 22) + 3;
      if ((n.x - x) ** 2 + (n.y - y) ** 2 <= r * r) return n;
    }
    return null;
  }

  function onMouseDown(ev: React.MouseEvent) {
    autoFitRef.current = false;
    const n = nodeAt(ev.clientX, ev.clientY);
    if (n) {
      draggingRef.current = { node: n, panning: false, lastX: ev.clientX, lastY: ev.clientY };
    } else {
      draggingRef.current = { node: null, panning: true, lastX: ev.clientX, lastY: ev.clientY };
    }
  }

  function onMouseMove(ev: React.MouseEvent) {
    const drag = draggingRef.current;
    if (drag.node) {
      const { x, y } = toWorld(ev.clientX, ev.clientY);
      drag.node.x = x;
      drag.node.y = y;
      requestDraw();
    } else if (drag.panning) {
      transformRef.current.offsetX += ev.clientX - drag.lastX;
      transformRef.current.offsetY += ev.clientY - drag.lastY;
      drag.lastX = ev.clientX;
      drag.lastY = ev.clientY;
      requestDraw();
    } else {
      const hit = nodeAt(ev.clientX, ev.clientY);
      if (hit !== hoverRef.current) {
        hoverRef.current = hit;
        requestDraw();
      }
    }
  }

  function onMouseUp(ev: React.MouseEvent) {
    const drag = draggingRef.current;
    if (drag.node) {
      const moved = Math.abs(ev.clientX - drag.lastX) + Math.abs(ev.clientY - drag.lastY);
      if (moved < 4) setSelected(drag.node.id);
    }
    draggingRef.current = { node: null, panning: false, lastX: 0, lastY: 0 };
    requestDraw();
  }

  function onWheel(ev: React.WheelEvent) {
    autoFitRef.current = false;
    const canvas = canvasRef.current!;
    const rect = canvas.getBoundingClientRect();
    const px = ev.clientX - rect.left;
    const py = ev.clientY - rect.top;
    const t = transformRef.current;
    const factor = ev.deltaY < 0 ? 1.1 : 1 / 1.1;
    const newScale = Math.min(10, Math.max(0.15, t.scale * factor));
    t.offsetX = px - ((px - t.offsetX) * newScale) / t.scale;
    t.offsetY = py - ((py - t.offsetY) * newScale) / t.scale;
    t.scale = newScale;
    requestDraw();
  }

  const nodesShown = loadedNodes;
  const totalNodes = meta?.total_nodes ?? 0;
  const loadingPct =
    meta && meta.node_count > 0 ? Math.round((loadedNodes / meta.node_count) * 100) : 0;

  return (
    <div className="graph-page">
      <div className="graph-toolbar">
        <div className="graph-toolbar-left">
          <strong>Graph</strong>
          {meta && (
            <span className="graph-meta">
              {nodesShown} of {totalNodes} nodes · {edgeCount} edges
              {meta.truncated && ' · showing top by degree'}
            </span>
          )}
        </div>
        <div className="graph-toolbar-right">
          <input
            type="search"
            className="graph-search"
            placeholder="Search nodes…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <label className="graph-limit">
            Kind:
            <select value={kindFilter} onChange={(e) => setKindFilter(e.target.value)}>
              <option value="">all</option>
              {kindOptions.map((k) => (
                <option key={k} value={k}>{k}</option>
              ))}
            </select>
          </label>
          {matchCount != null && (
            <span className="graph-meta">{matchCount} match{matchCount === 1 ? '' : 'es'}</span>
          )}
          <label className="graph-limit">
            Nodes:
            <select value={limit} onChange={(e) => setLimit(Number(e.target.value))}>
              <option value={200}>200</option>
              <option value={600}>600</option>
              <option value={1200}>1200</option>
              <option value={3000}>3000</option>
            </select>
          </label>
          <button
            type="button"
            className="btn-secondary"
            onClick={() => {
              autoFitRef.current = true;
              fitView(true);
              autoFitRef.current = false;
              requestDraw();
            }}
          >
            Zoom to core
          </button>
          <button type="button" className="btn-secondary" onClick={() => load(true)}>
            Recompute communities
          </button>
        </div>
      </div>

      <div className="graph-body">
        <div
          className="graph-canvas-wrap"
          ref={wrapRef}
          onMouseDown={onMouseDown}
          onMouseMove={onMouseMove}
          onMouseUp={onMouseUp}
          onMouseLeave={onMouseUp}
          onWheel={onWheel}
        >
          <canvas ref={canvasRef} className="graph-canvas" />
          {loading && (
            <div className="graph-overlay">
              <Spinner /> Streaming graph… {meta ? `${loadingPct}%` : ''}
            </div>
          )}
          {error && <div className="graph-overlay state-msg"><strong>Error</strong> {error}</div>}
          {!loading && !error && loadDone && loadedNodes === 0 && (
            <div className="graph-overlay">No nodes — run <code>ax index</code> first.</div>
          )}

          <div className="graph-legend">
            <div className="graph-legend-title">Communities</div>
            {communityLegend.map((c) => (
              <div key={c.id} className="graph-legend-row">
                <span className="graph-legend-swatch" style={{ background: c.color }} />
                <span className="graph-legend-label">{c.label}</span>
              </div>
            ))}
            <div className="graph-legend-title" style={{ marginTop: 8 }}>Edges</div>
            <div className="graph-legend-row"><span className="graph-legend-line solid" /> extracted</div>
            <div className="graph-legend-row"><span className="graph-legend-line dashed" /> inferred</div>
            <div className="graph-legend-row"><span className="graph-legend-line dotted" /> ambiguous</div>
            <div className="graph-legend-title" style={{ marginTop: 8 }}>Nodes</div>
            <div className="graph-legend-row"><span className="graph-legend-swatch" style={{ borderRadius: '50%', background: '#888' }} /> code</div>
            <div className="graph-legend-row"><span className="graph-legend-swatch" style={{ background: '#e0b341' }} /> doc</div>
          </div>
        </div>

        {selected && (
          <NodeDetailPanel
            nodeId={selected}
            variant="blade"
            onClose={() => setSelected(null)}
            onNavigate={(id) => setSelected(id)}
          />
        )}
      </div>
    </div>
  );
}
