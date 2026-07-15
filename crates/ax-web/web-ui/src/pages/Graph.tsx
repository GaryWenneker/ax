import { useEffect, useMemo, useRef, useState } from 'react';
import { streamGraph, type GraphNode, type GraphEdge, type GraphStreamMeta } from '../api';
import NodeDetailPanel from '../components/NodeDetail';
import { Spinner } from '../components/ui/Spinner';
import { usePersistedNumber } from '../hooks/usePersistedState';

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

const GRAPH_NODE_STEPS = [50, 100, 150, 200, 300, 400, 600] as const;
const DEFAULT_STEP_INDEX = 1; // 100 nodes

/** Visual radius from graph degree — kept small to reduce overlap. */
function nodeRadius(degree: number): number {
  return Math.min(1.0 + Math.sqrt(degree) * 0.5, 7);
}

const LABEL_FONT = '8px var(--font-mono, monospace)';

type GraphDetail = {
  maxIterations: number;
  maxEdgesDrawn: number;
  showLabels: boolean;
};

function detailForNodeLimit(limit: number): GraphDetail {
  if (limit <= 100) {
    return { maxIterations: 220, maxEdgesDrawn: 350, showLabels: true };
  }
  if (limit <= 150) {
    return { maxIterations: 280, maxEdgesDrawn: 500, showLabels: true };
  }
  if (limit <= 200) {
    return { maxIterations: 340, maxEdgesDrawn: 700, showLabels: true };
  }
  if (limit <= 300) {
    return { maxIterations: 420, maxEdgesDrawn: 1000, showLabels: false };
  }
  if (limit <= 400) {
    return { maxIterations: 500, maxEdgesDrawn: 1400, showLabels: false };
  }
  return { maxIterations: 600, maxEdgesDrawn: 2000, showLabels: false };
}

const DEFAULT_LIMIT = GRAPH_NODE_STEPS[DEFAULT_STEP_INDEX];

function useNarrowViewport(maxWidth = 768) {
  const [narrow, setNarrow] = useState(
    () => typeof window !== 'undefined' && window.matchMedia(`(max-width: ${maxWidth}px)`).matches,
  );
  useEffect(() => {
    const mq = window.matchMedia(`(max-width: ${maxWidth}px)`);
    const onChange = () => setNarrow(mq.matches);
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, [maxWidth]);
  return narrow;
}

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
  const pinchRef = useRef<{ active: boolean; dist: number; midX: number; midY: number }>({
    active: false,
    dist: 0,
    midX: 0,
    midY: 0,
  });
  const matchRef = useRef<Set<string> | null>(null);
  const detailRef = useRef<GraphDetail>(detailForNodeLimit(DEFAULT_LIMIT));

  const [stepIndex, setStepIndex] = usePersistedNumber(
    'graph-node-step',
    DEFAULT_STEP_INDEX,
    0,
    GRAPH_NODE_STEPS.length - 1,
  );
  const limit = GRAPH_NODE_STEPS[stepIndex];

  const [meta, setMeta] = useState<GraphStreamMeta | null>(null);
  const [loadedNodes, setLoadedNodes] = useState(0);
  const [edgeCount, setEdgeCount] = useState(0);
  const [legendNodes, setLegendNodes] = useState<GraphNode[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadDone, setLoadDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [search, setSearch] = useState('');
  const [kindFilter, setKindFilter] = useState('');
  const [communityFilter, setCommunityFilter] = useState('');
  const [matchCount, setMatchCount] = useState<number | null>(null);
  const isMobile = useNarrowViewport();

  useEffect(() => {
    detailRef.current = detailForNodeLimit(limit);
  }, [limit]);

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

  // Recenter the viewport on a single node without changing zoom too much.
  function centerOnNode(node: SimNode) {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.width / dpr;
    const h = canvas.height / dpr;
    const scale = Math.max(1.2, Math.min(transformRef.current.scale, 3));
    transformRef.current = { scale, offsetX: w / 2 - node.x * scale, offsetY: h / 2 - node.y * scale };
  }

  function applySearch(term: string, kind: string, community: string) {
    const q = term.trim().toLowerCase();
    const nodes = simNodesRef.current;
    if (!q && !kind && !community) {
      matchRef.current = null;
      setMatchCount(null);
      requestDraw();
      return;
    }
    const communityId = community ? Number(community) : null;
    const matches = nodes.filter((n) => {
      const nameHit = !q || n.name.toLowerCase().includes(q) || n.id.toLowerCase().includes(q);
      const kindHit = !kind || n.kind === kind;
      const communityHit = communityId == null || n.community_id === communityId;
      return nameHit && kindHit && communityHit;
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
      const maxIter = detailRef.current.maxIterations;
      const isDragging = draggingRef.current.node != null;

      const keepAlive = isDragging || iterationsRef.current < maxIter;
      if (keepAlive && nodes.length > 0) {
        const k = Math.sqrt((w * h) / Math.max(1, nodes.length)) * 1.8;
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

        // Attraction along edges (springs) — softer than repulsion for wider spacing.
        for (const e of edges) {
          const a = nodes[e.source];
          const b = nodes[e.target];
          if (!a || !b) continue;
          const dx = a.x - b.x;
          const dy = a.y - b.y;
          const dist = Math.sqrt(dx * dx + dy * dy) || 0.01;
          const force = (dist * dist) / k * 0.6;
          const fx = (dx / dist) * force;
          const fy = (dy / dist) * force;
          a.vx -= fx;
          a.vy -= fy;
          b.vx += fx;
          b.vy += fy;
        }

        // Gravity toward center + integrate with cooling.
        const cooling = isDragging ? 0.5 : 1 - iterationsRef.current / maxIter;
        const maxDisp = isDragging ? 15 : 30 * cooling + 1;
        for (const n of nodes) {
          n.vx += (cx - n.x) * 0.001;
          n.vy += (cy - n.y) * 0.001;
          if (draggingRef.current.node === n) {
            n.vx = 0;
            n.vy = 0;
            continue;
          }
          const disp = Math.sqrt(n.vx * n.vx + n.vy * n.vy) || 0.01;
          const limited = Math.min(disp, maxDisp);
          n.x += (n.vx / disp) * limited;
          n.y += (n.vy / disp) * limited;
          n.vx *= isDragging ? 0.7 : 0.85;
          n.vy *= isDragging ? 0.7 : 0.85;
        }
        iterationsRef.current++;
        draw();
        rafRef.current = requestAnimationFrame(step);
      } else {
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
    const detail = detailRef.current;
    const edgeStride =
      edges.length > detail.maxEdgesDrawn
        ? Math.ceil(edges.length / detail.maxEdgesDrawn)
        : 1;

    // Edges. When a search is active, only edges touching a match stay lit.
    ctx.lineWidth = 0.45;
    for (let ei = 0; ei < edges.length; ei++) {
      const e = edges[ei];
      const a = nodes[e.source];
      const b = nodes[e.target];
      if (!a || !b) continue;
      const highlight =
        hoverRef.current && (nodes[e.source] === hoverRef.current || nodes[e.target] === hoverRef.current);
      const dimmed = match != null && !isMatch(a) && !isMatch(b);
      if (ei % edgeStride !== 0 && !highlight && !dimmed && match == null) continue;
      ctx.beginPath();
      ctx.setLineDash(dashFor(e.confidence));
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
      const r = nodeRadius(n.degree);
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
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = '#fff';
        ctx.stroke();
      }
      const labelFocus =
        n === hoverRef.current || n.id === selected || (match != null && isMatch(n));
      if (!dimmed && (detail.showLabels || labelFocus)) {
        const fontSize = Math.max(6, Math.min(11, 8 / scale));
        ctx.fillStyle = 'rgba(230,230,230,0.85)';
        ctx.font = `${fontSize}px var(--font-mono, monospace)`;
        ctx.fillText(n.name, n.x + r + 1.5, n.y + 2);
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
    applySearch(search, kindFilter, communityFilter);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [search, kindFilter, communityFilter, loadDone]);

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

  function nodeAt(clientX: number, clientY: number, extraSlop = 0): SimNode | null {
    const { x, y } = toWorld(clientX, clientY);
    const nodes = simNodesRef.current;
    const { scale } = transformRef.current;
    const slop = extraSlop / scale;
    for (let i = nodes.length - 1; i >= 0; i--) {
      const n = nodes[i];
      const r = nodeRadius(n.degree) + 2 + slop;
      if ((n.x - x) ** 2 + (n.y - y) ** 2 <= r * r) return n;
    }
    return null;
  }

  function onPointerDown(ev: React.PointerEvent) {
    if (ev.button !== 0) return;
    ev.currentTarget.setPointerCapture(ev.pointerId);
    const touchSlop = ev.pointerType === 'touch' ? 12 : 0;
    const n = nodeAt(ev.clientX, ev.clientY, touchSlop);
    draggingRef.current = {
      node: n,
      panning: !n,
      lastX: ev.clientX,
      lastY: ev.clientY,
    };
    if (n) ensureSimulation();
  }

  function onPointerMove(ev: React.PointerEvent) {
    const drag = draggingRef.current;
    if (drag.node) {
      const { x, y } = toWorld(ev.clientX, ev.clientY);
      drag.node.x = x;
      drag.node.y = y;
      if (!runningRef.current) ensureSimulation();
    } else if (drag.panning) {
      transformRef.current.offsetX += ev.clientX - drag.lastX;
      transformRef.current.offsetY += ev.clientY - drag.lastY;
      drag.lastX = ev.clientX;
      drag.lastY = ev.clientY;
      requestDraw();
    } else if (ev.pointerType === 'mouse') {
      const hit = nodeAt(ev.clientX, ev.clientY);
      if (hit !== hoverRef.current) {
        hoverRef.current = hit;
        requestDraw();
      }
    }
  }

  function endPointerDrag(ev: React.PointerEvent) {
    const drag = draggingRef.current;
    if (drag.node) {
      const moved = Math.abs(ev.clientX - drag.lastX) + Math.abs(ev.clientY - drag.lastY);
      if (moved < (ev.pointerType === 'touch' ? 16 : 8)) setSelected(drag.node.id);
    }
    draggingRef.current = { node: null, panning: false, lastX: 0, lastY: 0 };
    if (ev.currentTarget.hasPointerCapture(ev.pointerId)) {
      ev.currentTarget.releasePointerCapture(ev.pointerId);
    }
    requestDraw();
  }

  function onWheel(ev: React.WheelEvent) {
    ev.preventDefault();
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

  function onTouchStart(ev: React.TouchEvent) {
    if (ev.touches.length === 2) {
      ev.preventDefault();
      const [a, b] = [ev.touches[0], ev.touches[1]];
      pinchRef.current = {
        active: true,
        dist: Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY),
        midX: (a.clientX + b.clientX) / 2,
        midY: (a.clientY + b.clientY) / 2,
      };
      draggingRef.current = { node: null, panning: false, lastX: 0, lastY: 0 };
    } else if (ev.touches.length === 1) {
      ev.preventDefault();
      const touch = ev.touches[0];
      const n = nodeAt(touch.clientX, touch.clientY, 16);
      draggingRef.current = { node: n, panning: !n, lastX: touch.clientX, lastY: touch.clientY };
      if (n) ensureSimulation();
    }
  }

  function onTouchMove(ev: React.TouchEvent) {
    if (ev.touches.length === 2 && pinchRef.current.active) {
      ev.preventDefault();
      const [a, b] = [ev.touches[0], ev.touches[1]];
      const newDist = Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
      const midX = (a.clientX + b.clientX) / 2;
      const midY = (a.clientY + b.clientY) / 2;
      const canvas = canvasRef.current;
      if (!canvas) return;
      const rect = canvas.getBoundingClientRect();
      const px = midX - rect.left;
      const py = midY - rect.top;
      const t = transformRef.current;
      const factor = newDist / (pinchRef.current.dist || 1);
      const newScale = Math.min(10, Math.max(0.15, t.scale * factor));
      t.offsetX = px - ((px - t.offsetX) * newScale) / t.scale;
      t.offsetY = py - ((py - t.offsetY) * newScale) / t.scale;
      t.offsetX += midX - pinchRef.current.midX;
      t.offsetY += midY - pinchRef.current.midY;
      t.scale = newScale;
      pinchRef.current.dist = newDist;
      pinchRef.current.midX = midX;
      pinchRef.current.midY = midY;
      requestDraw();
    } else if (ev.touches.length === 1 && !pinchRef.current.active) {
      ev.preventDefault();
      const touch = ev.touches[0];
      const drag = draggingRef.current;
      if (drag.node) {
        const { x, y } = toWorld(touch.clientX, touch.clientY);
        drag.node.x = x;
        drag.node.y = y;
        if (!runningRef.current) ensureSimulation();
      } else if (drag.panning) {
        transformRef.current.offsetX += touch.clientX - drag.lastX;
        transformRef.current.offsetY += touch.clientY - drag.lastY;
        drag.lastX = touch.clientX;
        drag.lastY = touch.clientY;
        requestDraw();
      }
    }
  }

  function onTouchEnd(ev: React.TouchEvent) {
    if (ev.touches.length < 2) {
      pinchRef.current.active = false;
    }
    if (ev.touches.length === 0) {
      const drag = draggingRef.current;
      if (drag.node) {
        const touch = ev.changedTouches[0];
        if (touch) {
          const moved = Math.abs(touch.clientX - drag.lastX) + Math.abs(touch.clientY - drag.lastY);
          if (moved < 16) setSelected(drag.node.id);
        }
      }
      draggingRef.current = { node: null, panning: false, lastX: 0, lastY: 0 };
      requestDraw();
    }
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
          <label className="graph-limit">
            Community:
            <select value={communityFilter} onChange={(e) => setCommunityFilter(e.target.value)}>
              <option value="">all</option>
              {communityLegend.map((c) => (
                <option key={c.id} value={c.id}>{c.label}</option>
              ))}
            </select>
          </label>
          {matchCount != null && (
            <span className="graph-meta">{matchCount} match{matchCount === 1 ? '' : 'es'}</span>
          )}
          <label className="graph-density" title="Lower density loads fewer nodes and edges for smoother interaction">
            <span className="graph-density-label">Density</span>
            <input
              type="range"
              className="graph-density-slider"
              min={0}
              max={GRAPH_NODE_STEPS.length - 1}
              step={1}
              value={stepIndex}
              onChange={(e) => setStepIndex(Number(e.target.value))}
            />
            <span className="graph-density-value">{limit} nodes</span>
          </label>
          {stepIndex >= 5 && (
            <span className="graph-meta graph-density-hint">High density — browser may lag</span>
          )}
          <button type="button" className="btn-secondary" onClick={() => load(true)}>
            Recompute communities
          </button>
        </div>
      </div>

      <div className="graph-body">
        <div
          className="graph-canvas-wrap"
          ref={wrapRef}
          style={{ touchAction: 'none' }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={endPointerDrag}
          onPointerCancel={endPointerDrag}
          onWheel={onWheel}
          onTouchStart={onTouchStart}
          onTouchMove={onTouchMove}
          onTouchEnd={onTouchEnd}
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
              <div
                key={c.id}
                className={`graph-legend-row${communityFilter === String(c.id) ? ' active' : ''}`}
                style={{ cursor: 'pointer' }}
                onClick={() => setCommunityFilter((prev) => prev === String(c.id) ? '' : String(c.id))}
              >
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
            variant={isMobile ? 'overlay' : 'blade'}
            onClose={() => setSelected(null)}
            onNavigate={(id) => setSelected(id)}
          />
        )}
      </div>
    </div>
  );
}
