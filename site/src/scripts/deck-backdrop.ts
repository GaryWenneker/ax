/**
 * Site backdrop, ported from the Ax presentation deck. Two independent layers, both fixed to the
 * viewport on every page:
 *
 * - Halftone grid: blue dots that breathe with slow interference waves. It is the page background:
 *   anchored to the viewport, the same on every page, and it does not follow the ring.
 * - Silk ring: each layer is a bundle of fine closed curves around one ring. The radius is
 *   modulated by two travelling waves; a per-line phase shift makes the lines cross like woven
 *   silk, and an envelope pinches the bundle into folds. Layers add up (`lighter`), a blurred
 *   low-res copy gives the haze, and sparkles ride the layers.
 *
 * Page changes move the ring like the deck's slide changes: the ring's state is kept in
 * sessionStorage on `pagehide`, the next page starts the ring where the last one left it, and
 * every layer springs to the new position on its own stiffness, so the layers leave one after
 * another, drift through each other and stretch into a drop along their direction of travel.
 *
 * Cost: trig is precomputed per curve segment and per grid row/column, the grid redraws at most
 * 30 times a second, the haze is blurred inside a small canvas, nothing uses CSS blend modes or
 * backdrop blur, and a frame-time monitor steps the quality down (fewer pixels, lines and
 * segments, then 30 fps) when frames run slow. Phones and Windows start lower.
 *
 * Motion follows html[data-ax-motion] (the visible Pause motion control), prefers-reduced-motion
 * and hidden tabs: when paused it draws one still frame and stops the loop. The light docs theme
 * hides the backdrop, and then nothing is drawn.
 */

const TAU = Math.PI * 2;
const ACCENT = '#8fbfff';
const STATE_KEY = 'ax-backdrop-state';
const STATE_MAX_AGE_MS = 15_000;

export type Variant = 'hero' | 'page';

export type Ribbon = {
  color: string;
  lines: number;
  alpha: number;
  off: number; // radial offset of the bundle (fraction of R)
  width: number; // bundle width (fraction of R)
  K: number; // spring stiffness: high = leads, low = trails like mist
  k1: number; a1: number; s1: number; d1: number;
  k2: number; a2: number; s2: number; d2: number;
  k3: number; s3: number;
  rot: number; // slow rotation of the whole layer (rad/s)
  p: number; // phase
};

export const RIBBONS: readonly Ribbon[] = [
  // wide, faint mist layers: the foggy body; they trail the furthest
  { color: '#6d28d9', lines: 40, alpha: 0.075, off: 0.0, width: 0.95, K: 2.6, k1: 1, a1: 0.09, s1: 0.11, d1: 1.2, k2: 3, a2: 0.05, s2: -0.16, d2: 2.0, k3: 2, s3: 0.09, rot: 0.02, p: 0.6 },
  { color: '#8b5cf6', lines: 36, alpha: 0.065, off: 0.06, width: 0.8, K: 3.6, k1: 2, a1: 0.08, s1: -0.13, d1: -1.6, k2: 4, a2: 0.04, s2: 0.19, d2: 1.5, k3: 3, s3: -0.1, rot: -0.025, p: 2.3 },
  // silk layers
  { color: '#5b21b6', lines: 32, alpha: 0.2, off: -0.05, width: 0.42, K: 4.8, k1: 1, a1: 0.08, s1: 0.14, d1: 1.1, k2: 6, a2: 0.026, s2: -0.38, d2: 3.2, k3: 4, s3: 0.17, rot: 0.02, p: 3.1 },
  { color: '#7c3aed', lines: 34, alpha: 0.19, off: 0.0, width: 0.36, K: 6.5, k1: 2, a1: 0.075, s1: 0.22, d1: 1.5, k2: 5, a2: 0.032, s2: 0.34, d2: 2.6, k3: 3, s3: 0.15, rot: 0.035, p: 0.0 },
  { color: '#a78bfa', lines: 30, alpha: 0.17, off: 0.03, width: 0.27, K: 8.5, k1: 3, a1: 0.06, s1: -0.19, d1: 2.2, k2: 4, a2: 0.038, s2: 0.29, d2: -1.8, k3: 2, s3: -0.12, rot: -0.03, p: 1.7 },
  { color: '#d946ef', lines: 20, alpha: 0.12, off: 0.05, width: 0.21, K: 11, k1: 2, a1: 0.068, s1: -0.25, d1: -2.0, k2: 7, a2: 0.02, s2: 0.42, d2: 1.4, k3: 3, s3: -0.2, rot: -0.045, p: 4.4 },
  { color: '#c4b5fd', lines: 18, alpha: 0.15, off: -0.01, width: 0.14, K: 14, k1: 4, a1: 0.045, s1: 0.27, d1: 2.8, k2: 3, a2: 0.044, s2: -0.2, d2: -2.4, k3: 2, s3: 0.14, rot: 0.055, p: 5.6 },
];

type Spark = { a: number; r: number; sp: number; tw: number; ph: number; size: number; layer: number };

/** Deterministic sparkles, so every load looks the same. Each one rides one layer. */
export function makeSparks(count: number): Spark[] {
  return Array.from({ length: count }, (_, i) => {
    const h = (n: number): number => {
      const x = Math.sin(i * 127.1 + n * 311.7) * 43758.5453;
      return x - Math.floor(x);
    };
    return { a: h(1) * TAU, r: (h(2) - 0.5) * 0.36, sp: 0.015 + h(3) * 0.05, tw: 0.5 + h(4) * 1.8, ph: h(5) * TAU, size: 0.6 + h(6) * 1.5, layer: i % RIBBONS.length };
  }).sort((a, b) => a.layer - b.layer);
}

/** Stable small number for a page path, so a page always gets the same composition. */
export function pageSeed(path: string): number {
  let hash = 0;
  for (const ch of path.replace(/\/+$/, '')) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return hash % 997;
}

export type Layout = {
  fx: number; // ring centre, fraction of viewport width
  fy: number; // ring centre, fraction of viewport height
  r: number; // ring radius, fraction of the short side
  k: number; // presence: 1 = hero, lower = quieter inner page
  spin: number; // extra rotation so pages do not repeat the same fold
};

/**
 * Where the ring sits. Hero: right of the copy on wide screens, up and off the edge on narrow ones.
 * Page: like the deck's inner slides, the centre walks around by the golden angle per page, but
 * stays on the right half on wide screens so the article column stays calm.
 */
export function backdropLayout(variant: Variant, w: number, h: number, seed = 0): Layout {
  if (variant === 'hero') {
    if (w < 600) return { fx: 0.92, fy: 0.16, r: 0.5, k: 1, spin: 0 };
    if (w < 900) return { fx: 0.9, fy: 0.3, r: 0.44, k: 1, spin: 0 };
    return { fx: 0.76, fy: 0.5, r: Math.min(0.4, (0.4 * (w / h)) / 1.6), k: 1, spin: 0 };
  }
  const a = seed * 2.399;
  const spin = (seed % 7) * 0.9;
  if (w < 900) return { fx: 0.75 + 0.2 * Math.cos(a), fy: 0.3 + 0.2 * Math.sin(a), r: 0.55, k: 0.45, spin };
  return { fx: 0.74 + 0.16 * Math.cos(a), fy: 0.5 + 0.3 * Math.sin(a), r: 0.48, k: 0.45, spin };
}

/**
 * Grid weight: strongest toward the right edge, calm behind the copy on the left. On tall
 * (portrait) screens the copy fills the width, so the grid gathers in the top-right corner.
 */
export function gridWeight(fx: number, fy: number, aspect: number): number {
  if (aspect < 1) {
    const dx = fx - 0.95;
    const dy = ((fy - 0.08) / aspect) * 0.55;
    return 1 - Math.sqrt(dx * dx + dy * dy) * 2;
  }
  const dx = (fx - 0.86) * aspect;
  const dy = fy - 0.45;
  return 1 - Math.sqrt(dx * dx + dy * dy) * 1.05;
}

/** The ring state carried from one page to the next. */
export type SavedState = {
  at: number; // Date.now() when saved
  clock: number; // animation seconds
  r: number;
  k: number;
  spin: number;
  bodies: { fx: number; fy: number; vx: number; vy: number }[];
};

export function readSavedState(raw: string | null, now: number): SavedState | null {
  if (!raw) return null;
  try {
    const s = JSON.parse(raw) as SavedState;
    if (typeof s.at !== 'number' || now - s.at > STATE_MAX_AGE_MS || now < s.at) return null;
    if (!Array.isArray(s.bodies) || s.bodies.length !== RIBBONS.length) return null;
    const finite = (n: unknown): boolean => typeof n === 'number' && Number.isFinite(n);
    if (![s.clock, s.r, s.k, s.spin].every(finite)) return null;
    if (!s.bodies.every((b) => [b.fx, b.fy, b.vx, b.vy].every(finite))) return null;
    return s;
  } catch {
    return null;
  }
}

/**
 * Quality tiers. The ring must stay sharp at rest, so the canvas never drops below screen
 * resolution (1 canvas pixel per CSS pixel); work is saved with fewer lines and curve points
 * first, and the lowest tier paces to a steady 30 fps.
 *   scale — canvas pixels per CSS pixel (capped by the device pixel ratio)
 *   px    — total canvas pixel budget; never pushes the scale below 1
 *   q     — share of silk lines drawn;  seg — points per curve;  every — draw every n-th frame
 */
export const TIERS = [
  { scale: 2, px: 6e6, q: 1, seg: 144, every: 1 },
  { scale: 1.5, px: 3.5e6, q: 0.75, seg: 120, every: 1 },
  { scale: 1, px: 2.5e6, q: 0.55, seg: 96, every: 1 },
  { scale: 1, px: 2.5e6, q: 0.42, seg: 80, every: 2 },
] as const;

/** Canvas pixels per CSS pixel for a tier: as sharp as the screen allows, at least 1×. */
export function canvasScale(tier: number, dpr: number, w: number, h: number): number {
  const t = TIERS[tier];
  return Math.max(1, Math.min(dpr, t.scale, Math.sqrt(t.px / (w * h))));
}

/** Starting tier: phones and Windows start one down, weak CPUs two, very low memory at the bottom. */
export function startTier(info: { ua: string; cores: number; memory: number; coarse: boolean; width: number }): number {
  let tier = 0;
  if (/Windows/i.test(info.ua)) tier = 1;
  if (info.coarse || info.width < 900) tier = Math.max(tier, 1);
  if (info.cores > 0 && info.cores <= 4) tier = Math.max(tier, 2);
  if (info.memory > 0 && info.memory <= 2) tier = 3;
  return tier;
}

export type Monitor = { n: number; sum: number; spikes: number };

/**
 * Frame-time monitor. Feeds one frame time (seconds per display frame) and returns the tier to
 * use. Every 30 frames: step down on a slow average (under ~54 fps) or repeated spikes (dropped
 * frames read as stutter even when the average is fine); every 90 calm, fast frames: step back
 * up, but never into a tier that already proved too slow on this page.
 */
export function judgeFrames(m: Monitor, frame: number, tier: number, tooSlow: Set<number>): number {
  m.n++;
  m.sum += frame;
  if (frame > 0.028) m.spikes++;
  const avg = m.sum / m.n;
  if (m.n >= 30 && (avg > 0.0185 || m.spikes >= 4)) {
    Object.assign(m, { n: 0, sum: 0, spikes: 0 });
    return Math.min(TIERS.length - 1, tier + 1);
  }
  if (m.n >= 90) {
    Object.assign(m, { n: 0, sum: 0, spikes: 0 });
    if (avg < 0.0135 && tier > 0 && !tooSlow.has(tier - 1)) return tier - 1;
  }
  return tier;
}

/** Per-segment cos/sin of k·θ for k = 0..8, so curves need no trig per point. */
function segmentTables(seg: number): { cos: Float32Array[]; sin: Float32Array[] } {
  const cos: Float32Array[] = [];
  const sin: Float32Array[] = [];
  for (let k = 0; k <= 8; k++) {
    const c = new Float32Array(seg + 1);
    const s = new Float32Array(seg + 1);
    for (let i = 0; i <= seg; i++) {
      const th = (i / seg) * TAU * k;
      c[i] = Math.cos(th);
      s[i] = Math.sin(th);
    }
    cos.push(c);
    sin.push(s);
  }
  return { cos, sin };
}

type Body = { fx: number; fy: number; vx: number; vy: number; dx: number; dy: number };

export function mountDeckBackdrop(root: HTMLElement): () => void {
  const grid = root.querySelector<HTMLCanvasElement>('[data-ax-backdrop-dots]');
  const silk = root.querySelector<HTMLCanvasElement>('[data-ax-silk]');
  const fog = root.querySelector<HTMLCanvasElement>('[data-ax-silk-fog]');
  const gctx = grid?.getContext('2d');
  const ctx = silk?.getContext('2d');
  const fctx = fog?.getContext('2d');
  if (!grid || !silk || !fog || !gctx || !ctx || !fctx) return () => {};

  const variant: Variant = root.dataset.axVariant === 'page' ? 'page' : 'hero';
  const seed = pageSeed(location.pathname);
  const html = document.documentElement;
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)');
  const coarse = window.matchMedia('(pointer: coarse)').matches;
  const nav = navigator as Navigator & { deviceMemory?: number };
  let tier = startTier({ ua: navigator.userAgent, cores: navigator.hardwareConcurrency || 0, memory: nav.deviceMemory || 0, coarse, width: window.innerWidth });
  const dpr = window.devicePixelRatio || 1;
  const sparks = makeSparks(coarse ? 60 : 90);
  // Canvas filters blur the haze inside its own small canvas; without them (older Safari) the
  // haze canvas is simply made tiny, and scaling it up blurs it for free.
  const nativeBlur = typeof fctx.filter === 'string' && fctx.filter === 'none';

  let w = 0;
  let h = 0;
  let tables = segmentTables(TIERS[tier].seg);
  let raf = 0;
  let last = performance.now();
  const monitor: Monitor = { n: 0, sum: 0, spikes: 0 };
  const tooSlow = new Set<number>();
  const mountedAt = performance.now();
  let gridAt = -1;
  let tick = 0;

  // Grid geometry, rebuilt on resize: dot centres, base weight and per-row/column phases.
  let cols = new Float32Array(0);
  let rows = new Float32Array(0);
  let base = new Float32Array(0);
  let sp = 16;

  const resize = (): void => {
    w = Math.max(1, window.innerWidth);
    h = Math.max(1, window.innerHeight);
    // Round dots stay crisp at 1–1.5×; the grid only redraws at 30 fps anyway.
    const gs = Math.max(1, Math.min(dpr, 1.5, Math.sqrt(3e6 / (w * h))));
    grid.width = Math.ceil(w * gs);
    grid.height = Math.ceil(h * gs);
    gctx.setTransform(gs, 0, 0, gs, 0, 0);
    const ss = canvasScale(tier, dpr, w, h);
    root.dataset.axTier = String(tier);
    silk.width = Math.ceil(w * ss);
    silk.height = Math.ceil(h * ss);
    ctx.setTransform(ss, 0, 0, ss, 0, 0);
    const fs = nativeBlur ? Math.min(0.3, Math.sqrt(1.2e5 / (w * h))) : Math.min(0.08, Math.sqrt(9e3 / (w * h)));
    fog.width = Math.max(1, Math.ceil(w * fs));
    fog.height = Math.max(1, Math.ceil(h * fs));

    sp = Math.max(14, Math.round(w / 96));
    const nx = Math.ceil(w / sp);
    const ny = Math.ceil(h / sp);
    cols = new Float32Array(nx);
    rows = new Float32Array(ny);
    for (let i = 0; i < nx; i++) cols[i] = sp / 2 + i * sp;
    for (let j = 0; j < ny; j++) rows[j] = sp / 2 + j * sp;
    base = new Float32Array(nx * ny);
    const aspect = w / h;
    for (let j = 0; j < ny; j++) for (let i = 0; i < nx; i++) base[j * nx + i] = gridWeight(cols[i] / w, rows[j] / h, aspect);
    gridAt = -1;
  };

  // --- halftone grid: fixed to the viewport, breathing in place ---
  const drawGrid = (t: number): void => {
    gctx.clearRect(0, 0, w, h);
    gctx.fillStyle = ACCENT;
    gctx.globalAlpha = 0.26;
    gctx.beginPath();
    const nx = cols.length;
    const ny = rows.length;
    // sin(x·a + t·b) + sin(y·c − t·d) + sin((x + y)·e + t·f); the last term is expanded with the
    // angle-sum identity so every term is one value per column or row.
    const sx = new Float32Array(nx);
    const ex = new Float32Array(nx);
    const cx = new Float32Array(nx);
    for (let i = 0; i < nx; i++) {
      sx[i] = Math.sin(cols[i] * 0.011 + t * 0.6);
      ex[i] = Math.sin(cols[i] * 0.007 + t * 0.3);
      cx[i] = Math.cos(cols[i] * 0.007 + t * 0.3);
    }
    const rmax = sp * 0.38;
    for (let j = 0; j < ny; j++) {
      const y = rows[j];
      const sy = Math.sin(y * 0.014 - t * 0.45);
      const ey = Math.sin(y * 0.007);
      const cy = Math.cos(y * 0.007);
      const row = j * nx;
      for (let i = 0; i < nx; i++) {
        const v = base[row + i] + 0.2 * (sx[i] + sy + ex[i] * cy + cx[i] * ey);
        if (v <= 0.04) continue;
        const r = (v < 1 ? v : 1) * rmax;
        const x = cols[i];
        gctx.moveTo(x + r, y);
        gctx.arc(x, y, r, 0, TAU);
      }
    }
    gctx.fill();
  };

  // --- ring state ---
  const target = (): Layout => backdropLayout(variant, w, h, seed);
  resize();
  const goal = target();
  const saved = readSavedState(safeGet(), Date.now());
  const cur = { r: goal.r, k: goal.k, spin: goal.spin };
  let clock = 8; // seconds of animation time; starts mid-motion so the first frame is already woven
  const bodies: Body[] = RIBBONS.map(() => ({ fx: goal.fx, fy: goal.fy, vx: 0, vy: 0, dx: 0, dy: 0 }));
  if (saved) {
    // Continue where the previous page left the ring, then spring to this page's position.
    clock = saved.clock + Math.min(2, (Date.now() - saved.at) / 1000);
    cur.r = saved.r;
    cur.k = saved.k;
    cur.spin = saved.spin;
    saved.bodies.forEach((b, i) => Object.assign(bodies[i], b, { dx: b.vx, dy: b.vy }));
  }

  const save = (): void => {
    const state: SavedState = {
      at: Date.now(),
      clock,
      r: cur.r,
      k: cur.k,
      spin: cur.spin,
      bodies: bodies.map(({ fx, fy, vx, vy }) => ({ fx, fy, vx, vy })),
    };
    try { sessionStorage.setItem(STATE_KEY, JSON.stringify(state)); } catch { /* private mode */ }
  };

  const step = (frame: number): void => {
    const tg = target();
    // Paused or reduced motion: no travel, the ring simply sits at this page's position.
    const still = html.dataset.axMotion === 'paused' || (reduced.matches && html.dataset.axMotion !== 'running');
    const ease = still ? 1 : 1 - Math.exp(-frame * 1.9);
    cur.r += (tg.r - cur.r) * ease;
    cur.k += (tg.k - cur.k) * ease;
    cur.spin += (tg.spin - cur.spin) * ease;
    // Each layer follows the target on its own soft spring (fixed sub-steps keep it stable).
    const steps = Math.max(1, Math.ceil(frame / (1 / 60)));
    const dt = frame / steps;
    for (let i = 0; i < bodies.length; i++) {
      const b = bodies[i];
      if (still) {
        Object.assign(b, { fx: tg.fx, fy: tg.fy, vx: 0, vy: 0, dx: 0, dy: 0 });
        continue;
      }
      const K = RIBBONS[i].K;
      const D = 2 * 0.8 * Math.sqrt(K);
      for (let n = 0; n < steps; n++) {
        b.vx += (K * (tg.fx - b.fx) - D * b.vx) * dt;
        b.vy += (K * (tg.fy - b.fy) - D * b.vy) * dt;
        b.fx += b.vx * dt;
        b.fy += b.vy * dt;
        // the stretch eases in and out behind the motion, so nothing snaps
        const lag = Math.min(1, dt * 3.2);
        b.dx += (b.vx - b.dx) * lag;
        b.dy += (b.vy - b.dy) * lag;
      }
    }
  };

  const drawRing = (t: number): void => {
    const { q, seg } = TIERS[tier];
    const m = Math.min(w, h);
    const R = m * cur.r;
    const strength = 0.75 + 0.25 * cur.k;
    const { cos: TC, sin: TS } = tables;

    ctx.globalCompositeOperation = 'source-over';
    ctx.clearRect(0, 0, w, h);
    ctx.globalCompositeOperation = 'lighter';
    ctx.lineWidth = 1;

    // Droplet morph per layer: stretch along its own direction of travel, taper the trailing side.
    let cx = 0, cy = 0, ux = 1, uy = 0, st = 0;
    const aim = (b: Body): void => {
      cx = b.fx * w;
      cy = b.fy * h;
      const dvx = (b.dx * w) / m;
      const dvy = (b.dy * h) / m;
      const dmag = Math.hypot(dvx, dvy);
      st = Math.min(0.7, dmag * 0.55);
      ux = dmag > 1e-4 ? dvx / dmag : 1;
      uy = dmag > 1e-4 ? dvy / dmag : 0;
    };
    // place(px, py) → the point after the droplet morph; inlined math keeps it allocation-free.
    let ox = 0, oy = 0;
    const place = (px: number, py: number): void => {
      if (st < 1e-3) { ox = cx + px; oy = cy + py; return; }
      const a = px * ux + py * uy; // along travel
      const b = -px * uy + py * ux; // across travel
      const qn = Math.max(-1.3, Math.min(1.3, a / R));
      const a2 = a * (1 + st * (0.8 - 0.55 * qn));
      const b2 = b * (1 - st * 0.25) * (1 - st * 0.3 * Math.max(0, -qn));
      ox = cx + a2 * ux - b2 * uy;
      oy = cy + a2 * uy + b2 * ux;
    };

    for (let li = 0; li < RIBBONS.length; li++) {
      const rb = RIBBONS[li];
      aim(bodies[li]);
      ctx.strokeStyle = rb.color;
      const lines = Math.max(6, Math.round(rb.lines * q));
      // fewer lines on a lower tier, each a little brighter, so the ring keeps its weight
      ctx.globalAlpha = Math.min(1, (rb.alpha * strength * rb.lines) / lines);
      const rot = rb.p + cur.spin * 0.6 + t * rb.rot;
      const cr = Math.cos(rot);
      const sr = Math.sin(rot);
      const breathe = R * (1 + 0.035 * Math.sin(t * 0.23 + rb.p * 1.7));
      // envelope phase is the same for every line of the layer
      const pe = rb.p * 2 + t * rb.s3;
      const ce = Math.cos(pe), se = Math.sin(pe);
      const c1 = TC[rb.k1], s1 = TS[rb.k1], c2 = TC[rb.k2], s2 = TS[rb.k2], c3 = TC[rb.k3], s3 = TS[rb.k3];
      const c0 = TC[1], s0 = TS[1];
      for (let j = 0; j < lines; j++) {
        const u = j / (lines - 1);
        const p1 = rb.p + t * rb.s1 + u * rb.d1;
        const p2 = -rb.p + t * rb.s2 + u * rb.d2;
        const cp1 = Math.cos(p1), sp1 = Math.sin(p1), cp2 = Math.cos(p2), sp2 = Math.sin(p2);
        const spread = rb.width * (u - 0.5);
        ctx.beginPath();
        for (let s = 0; s <= seg; s++) {
          // sin(k·θ + φ) = sin(kθ)·cos φ + cos(kθ)·sin φ, with kθ from the tables
          const env = 0.5 + 0.5 * (s3[s] * ce + c3[s] * se);
          const rr =
            breathe *
            (1 + rb.off + spread * (0.25 + 0.75 * env) + rb.a1 * (s1[s] * cp1 + c1[s] * sp1) + rb.a2 * (s2[s] * cp2 + c2[s] * sp2));
          // rotate (cos θ, sin θ) by the layer rotation
          const ct = c0[s] * cr - s0[s] * sr;
          const stt = s0[s] * cr + c0[s] * sr;
          place(rr * ct, rr * stt);
          if (s === 0) ctx.moveTo(ox, oy);
          else ctx.lineTo(ox, oy);
        }
        ctx.stroke();
      }
    }

    // --- sparkles ---
    ctx.fillStyle = '#ede9fe';
    let aimed = -1;
    for (const sk of sparks) {
      if (sk.layer !== aimed) {
        aimed = sk.layer;
        aim(bodies[aimed]);
      }
      const a = sk.a + t * sk.sp + cur.spin * 0.6;
      const rr = R * (1 + sk.r + 0.03 * Math.sin(t * 0.4 + sk.ph));
      place(rr * Math.cos(a), rr * Math.sin(a));
      if (ox < -10 || oy < -10 || ox > w + 10 || oy > h + 10) continue;
      const tw = 0.5 + 0.5 * Math.sin(t * sk.tw + sk.ph);
      ctx.globalAlpha = 0.12 * tw * strength;
      ctx.beginPath();
      ctx.arc(ox, oy, sk.size * 3.2, 0, TAU);
      ctx.fill();
      ctx.globalAlpha = (0.35 + 0.65 * tw) * strength;
      ctx.beginPath();
      ctx.arc(ox, oy, sk.size, 0, TAU);
      ctx.fill();
    }
    ctx.globalAlpha = 1;

    // --- haze: a low-res, blurred copy of the silk ---
    fctx.clearRect(0, 0, fog.width, fog.height);
    if (nativeBlur) fctx.filter = `blur(${(fog.width * 0.014).toFixed(1)}px)`;
    fctx.drawImage(silk, 0, 0, fog.width, fog.height);
  };

  const render = (t: number, frame: number): void => {
    step(frame);
    // The grid breathes slowly; 30 redraws a second is plenty and halves its cost.
    if (gridAt < 0 || t - gridAt >= 1 / 30) {
      drawGrid(t);
      gridAt = t;
    }
    drawRing(t);
  };

  // Hidden by the light theme (display:none) → nothing to draw.
  const visible = (): boolean => root.getClientRects().length > 0;
  const paused = (): boolean => html.dataset.axMotion === 'paused' || document.hidden;

  const loop = (now: number): void => {
    const raw = (now - last) / 1000;
    raf = paused() || !visible() ? 0 : requestAnimationFrame(loop);
    // The lowest tier renders every other display frame: a steady 30 fps beats a ragged 45.
    if (++tick % TIERS[tier].every !== 0) return;
    last = now;
    const frame = Math.min(0.1, raw);
    clock += frame;

    // Measure real frame time. The page load itself is janky (fonts, layout, other scripts), so
    // the first 1.5 s never count against the backdrop.
    if (now - mountedAt > 1500 && raw < 0.25) {
      const decision = judgeFrames(monitor, raw / TIERS[tier].every, tier, tooSlow);
      if (decision !== tier) {
        if (decision > tier) tooSlow.add(tier);
        tier = decision;
        tables = segmentTables(TIERS[tier].seg);
        resize();
      }
    }
    render(clock, frame);
  };

  const sync = (): void => {
    if (!visible()) {
      cancelAnimationFrame(raf);
      raf = 0;
    } else if (paused()) {
      cancelAnimationFrame(raf);
      raf = 0;
      render(clock, 0.1);
    } else if (!raf) {
      last = performance.now();
      raf = requestAnimationFrame(loop);
    }
  };

  const onResize = (): void => {
    if (!visible()) return;
    resize();
    if (!raf) render(clock, 0);
  };

  if (visible()) render(clock, 0);
  sync();
  window.addEventListener('resize', onResize);
  // The theme picker and the motion control both flip attributes on <html>.
  const attributes = new MutationObserver(() => {
    onResize();
    sync();
  });
  attributes.observe(html, { attributes: true, attributeFilter: ['data-theme', 'data-ax-motion'] });
  window.addEventListener('ax:motion', sync);
  document.addEventListener('visibilitychange', sync);
  window.addEventListener('pagehide', save);

  return () => {
    save();
    cancelAnimationFrame(raf);
    raf = 0;
    attributes.disconnect();
    window.removeEventListener('resize', onResize);
    window.removeEventListener('ax:motion', sync);
    document.removeEventListener('visibilitychange', sync);
    window.removeEventListener('pagehide', save);
  };
}

function safeGet(): string | null {
  try {
    return sessionStorage.getItem(STATE_KEY);
  } catch {
    return null;
  }
}
