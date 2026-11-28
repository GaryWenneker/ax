// The halftone dot grid behind the website: accent dots that breathe with slow
// interference waves. The same grid as ax-admin (crates/ax-admin/web-ui/src/gridBackdrop.ts),
// kept apart so the site does not depend on the admin's code. No ring, no other effects.

export interface Grid {
  spacing: number;
  cols: Float32Array;
  rows: Float32Array;
  /** Base weight per dot, row by row. */
  base: Float32Array;
}

/**
 * Grid weight: strongest toward the right edge, calm on the left. On tall
 * (portrait) screens the grid gathers in the top-right corner.
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

export function buildGrid(w: number, h: number): Grid {
  const spacing = Math.max(14, Math.round(w / 96));
  const nx = Math.ceil(w / spacing);
  const ny = Math.ceil(h / spacing);
  const cols = new Float32Array(nx);
  const rows = new Float32Array(ny);
  for (let i = 0; i < nx; i++) cols[i] = spacing / 2 + i * spacing;
  for (let j = 0; j < ny; j++) rows[j] = spacing / 2 + j * spacing;
  const base = new Float32Array(nx * ny);
  const aspect = w / h;
  for (let j = 0; j < ny; j++) {
    for (let i = 0; i < nx; i++) base[j * nx + i] = gridWeight((cols[i] ?? 0) / w, (rows[j] ?? 0) / h, aspect);
  }
  return { spacing, cols, rows, base };
}

/**
 * Dot radii at time `t` (seconds), row by row; 0 means no dot. The waves are
 * sin(x·a + t·b) + sin(y·c − t·d) + sin((x + y)·e + t·f), with the last term
 * expanded so every term is one value per column or row.
 */
export function dotRadii(grid: Grid, t: number, out?: Float32Array): Float32Array {
  const { cols, rows, base, spacing } = grid;
  const nx = cols.length;
  const ny = rows.length;
  const radii = out && out.length === nx * ny ? out : new Float32Array(nx * ny);
  const sx = new Float32Array(nx);
  const ex = new Float32Array(nx);
  const cx = new Float32Array(nx);
  for (let i = 0; i < nx; i++) {
    const x = cols[i] ?? 0;
    sx[i] = Math.sin(x * 0.011 + t * 0.6);
    ex[i] = Math.sin(x * 0.007 + t * 0.3);
    cx[i] = Math.cos(x * 0.007 + t * 0.3);
  }
  const rmax = spacing * 0.38;
  for (let j = 0; j < ny; j++) {
    const y = rows[j] ?? 0;
    const sy = Math.sin(y * 0.014 - t * 0.45);
    const ey = Math.sin(y * 0.007);
    const cy = Math.cos(y * 0.007);
    const row = j * nx;
    for (let i = 0; i < nx; i++) {
      const v = (base[row + i] ?? 0) + 0.2 * ((sx[i] ?? 0) + sy + (ex[i] ?? 0) * cy + (cx[i] ?? 0) * ey);
      radii[row + i] = v <= 0.04 ? 0 : (v < 1 ? v : 1) * rmax;
    }
  }
  return radii;
}

const TAU = Math.PI * 2;
/** The grid breathes slowly; 30 redraws a second is plenty. */
const FRAME_MS = 1000 / 30;

/**
 * Draws the grid on the canvas inside `root` and keeps it breathing. Decorative only:
 * it stands still with reduced motion and pauses while the tab is hidden.
 * Returns a function that stops it.
 */
export function mountGridBackdrop(root: HTMLElement): () => void {
  const el = root.querySelector('canvas');
  const ctx = el?.getContext('2d');
  if (!el || !ctx) return () => {};
  const readColor = () => getComputedStyle(el).color || '#88c0d0';
  let color = readColor();
  const reduce = window.matchMedia('(prefers-reduced-motion: reduce)');
  let grid = buildGrid(1, 1);
  let radii: Float32Array = new Float32Array(0);
  let w = 1;
  let h = 1;
  let frame = 0;
  let last = -Infinity;
  const start = performance.now();

  const draw = (t: number) => {
    radii = dotRadii(grid, t, radii);
    ctx.clearRect(0, 0, w, h);
    ctx.fillStyle = color;
    ctx.globalAlpha = 0.26;
    ctx.beginPath();
    const nx = grid.cols.length;
    for (let j = 0; j < grid.rows.length; j++) {
      const y = grid.rows[j] ?? 0;
      for (let i = 0; i < nx; i++) {
        const r = radii[j * nx + i] ?? 0;
        if (r === 0) continue;
        const x = grid.cols[i] ?? 0;
        ctx.moveTo(x + r, y);
        ctx.arc(x, y, r, 0, TAU);
      }
    }
    ctx.fill();
  };

  const resize = () => {
    w = Math.max(1, window.innerWidth);
    h = Math.max(1, window.innerHeight);
    // Round dots stay crisp at 1–1.5×.
    const scale = Math.max(1, Math.min(window.devicePixelRatio || 1, 1.5, Math.sqrt(3e6 / (w * h))));
    el.width = Math.ceil(w * scale);
    el.height = Math.ceil(h * scale);
    ctx.setTransform(scale, 0, 0, scale, 0, 0);
    grid = buildGrid(w, h);
    color = readColor();
    draw((performance.now() - start) / 1000);
  };

  const schedule = () => {
    if (frame === 0 && !reduce.matches && document.visibilityState === 'visible') frame = requestAnimationFrame(loop);
  };
  const loop = (now: number) => {
    frame = 0;
    if (now - last >= FRAME_MS) {
      last = now;
      draw((now - start) / 1000);
    }
    schedule();
  };
  const stop = () => {
    if (frame !== 0) cancelAnimationFrame(frame);
    frame = 0;
  };
  const onChange = () => {
    stop();
    schedule();
  };

  resize();
  schedule();
  window.addEventListener('resize', resize);
  document.addEventListener('visibilitychange', onChange);
  reduce.addEventListener('change', onChange);
  return () => {
    stop();
    window.removeEventListener('resize', resize);
    document.removeEventListener('visibilitychange', onChange);
    reduce.removeEventListener('change', onChange);
  };
}
