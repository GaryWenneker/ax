/**
 * Violet silk ring for the homepage backdrop, ported from the Ax presentation deck.
 *
 * Each layer is a bundle of fine closed curves around one ring. The ring radius is modulated by
 * two travelling waves; a per-line phase shift makes the lines cross like woven silk, and an
 * envelope pinches the bundle into folds. Layers add up (`lighter`), a blurred low-res copy gives
 * the haze, and sparkles ride the layers.
 *
 * Motion follows html[data-ax-motion] (the visible Pause motion control) and
 * prefers-reduced-motion: when paused it draws one still frame and stops the loop.
 */

const TAU = Math.PI * 2;

export type Ribbon = {
  color: string;
  lines: number;
  alpha: number;
  off: number; // radial offset of the bundle (fraction of R)
  width: number; // bundle width (fraction of R)
  k1: number; a1: number; s1: number; d1: number;
  k2: number; a2: number; s2: number; d2: number;
  k3: number; s3: number;
  rot: number; // slow rotation of the whole layer (rad/s)
  p: number; // phase
};

export const RIBBONS: readonly Ribbon[] = [
  // wide, faint mist layers: the foggy body
  { color: '#6d28d9', lines: 40, alpha: 0.075, off: 0.0, width: 0.95, k1: 1, a1: 0.09, s1: 0.11, d1: 1.2, k2: 3, a2: 0.05, s2: -0.16, d2: 2.0, k3: 2, s3: 0.09, rot: 0.02, p: 0.6 },
  { color: '#8b5cf6', lines: 36, alpha: 0.065, off: 0.06, width: 0.8, k1: 2, a1: 0.08, s1: -0.13, d1: -1.6, k2: 4, a2: 0.04, s2: 0.19, d2: 1.5, k3: 3, s3: -0.1, rot: -0.025, p: 2.3 },
  // silk layers
  { color: '#5b21b6', lines: 32, alpha: 0.2, off: -0.05, width: 0.42, k1: 1, a1: 0.08, s1: 0.14, d1: 1.1, k2: 6, a2: 0.026, s2: -0.38, d2: 3.2, k3: 4, s3: 0.17, rot: 0.02, p: 3.1 },
  { color: '#7c3aed', lines: 34, alpha: 0.19, off: 0.0, width: 0.36, k1: 2, a1: 0.075, s1: 0.22, d1: 1.5, k2: 5, a2: 0.032, s2: 0.34, d2: 2.6, k3: 3, s3: 0.15, rot: 0.035, p: 0.0 },
  { color: '#a78bfa', lines: 30, alpha: 0.17, off: 0.03, width: 0.27, k1: 3, a1: 0.06, s1: -0.19, d1: 2.2, k2: 4, a2: 0.038, s2: 0.29, d2: -1.8, k3: 2, s3: -0.12, rot: -0.03, p: 1.7 },
  { color: '#d946ef', lines: 20, alpha: 0.12, off: 0.05, width: 0.21, k1: 2, a1: 0.068, s1: -0.25, d1: -2.0, k2: 7, a2: 0.02, s2: 0.42, d2: 1.4, k3: 3, s3: -0.2, rot: -0.045, p: 4.4 },
  { color: '#c4b5fd', lines: 18, alpha: 0.15, off: -0.01, width: 0.14, k1: 4, a1: 0.045, s1: 0.27, d1: 2.8, k2: 3, a2: 0.044, s2: -0.2, d2: -2.4, k3: 2, s3: 0.14, rot: 0.055, p: 5.6 },
];

type Spark = { a: number; r: number; sp: number; tw: number; ph: number; size: number };

/** Deterministic sparkles, so every load looks the same. */
export function makeSparks(count: number): Spark[] {
  return Array.from({ length: count }, (_, i) => {
    const h = (n: number): number => {
      const x = Math.sin(i * 127.1 + n * 311.7) * 43758.5453;
      return x - Math.floor(x);
    };
    return { a: h(1) * TAU, r: (h(2) - 0.5) * 0.36, sp: 0.015 + h(3) * 0.05, tw: 0.5 + h(4) * 1.8, ph: h(5) * TAU, size: 0.6 + h(6) * 1.5 };
  });
}

/**
 * Ring centre (fractions of the viewport) and radius (fraction of the short side).
 * Wide screens keep the ring to the right of the copy; narrow screens push it up and off the edge.
 */
export function ringLayout(w: number, h: number): { fx: number; fy: number; r: number } {
  if (w < 600) return { fx: 0.92, fy: 0.16, r: 0.5 };
  if (w < 900) return { fx: 0.9, fy: 0.3, r: 0.44 };
  return { fx: 0.76, fy: 0.5, r: Math.min(0.4, 0.4 * (w / h) / 1.6) };
}

// Pixel budgets per quality tier. A slow frame rate steps down a tier: fewer pixels, lines and segments.
const TIERS = [
  { px: 1.6e6, q: 1, seg: 150 },
  { px: 0.9e6, q: 0.7, seg: 120 },
  { px: 0.5e6, q: 0.5, seg: 96 },
  { px: 0.3e6, q: 0.36, seg: 80 },
] as const;

export function mountSilkRing(root: HTMLElement): () => void {
  const silk = root.querySelector<HTMLCanvasElement>('[data-ax-silk]');
  const fog = root.querySelector<HTMLCanvasElement>('[data-ax-silk-fog]');
  const ctx = silk?.getContext('2d');
  const fctx = fog?.getContext('2d');
  if (!silk || !fog || !ctx || !fctx) return () => {};

  const sparks = makeSparks(90);
  const html = document.documentElement;
  const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
  const nativeBlur = typeof fctx.filter === 'string';
  fog.classList.toggle('ax-silk-fog-native', nativeBlur);

  let tier = /Windows/i.test(navigator.userAgent) ? 1 : 0;
  let w = 0;
  let h = 0;
  let raf = 0;
  let last = performance.now();
  let clock = 8; // seconds of animation time; starts mid-motion so the first frame is already woven
  let frames = 0;
  let slowAcc = 0;
  let slowN = 0;

  const resize = (): void => {
    // The root can still be 0×0 before the stylesheet applies; fall back to the viewport.
    w = Math.max(1, root.clientWidth || window.innerWidth);
    h = Math.max(1, root.clientHeight || window.innerHeight);
    const ss = Math.min(dpr, Math.sqrt(TIERS[tier].px / (w * h)));
    silk.width = Math.ceil(w * ss);
    silk.height = Math.ceil(h * ss);
    ctx.setTransform(ss, 0, 0, ss, 0, 0);
    const fs = Math.min(0.3, Math.sqrt(1.2e5 / (w * h)));
    fog.width = Math.ceil(w * fs);
    fog.height = Math.ceil(h * fs);
  };

  const render = (t: number): void => {
    const { q, seg } = TIERS[tier];
    const layout = ringLayout(w, h);
    const m = Math.min(w, h);
    const R = m * layout.r;
    const cx = layout.fx * w;
    const cy = layout.fy * h;

    ctx.globalCompositeOperation = 'source-over';
    ctx.clearRect(0, 0, w, h);
    ctx.globalCompositeOperation = 'lighter';
    ctx.lineWidth = 1;

    for (const rb of RIBBONS) {
      ctx.strokeStyle = rb.color;
      const lines = Math.max(6, Math.round(rb.lines * q));
      // fewer lines on a lower tier, each a little brighter, so the ring keeps its weight
      ctx.globalAlpha = Math.min(1, (rb.alpha * rb.lines) / lines);
      const base = rb.p + t * rb.rot;
      const breathe = 1 + 0.035 * Math.sin(t * 0.23 + rb.p * 1.7);
      for (let j = 0; j < lines; j++) {
        const u = j / (lines - 1);
        ctx.beginPath();
        for (let s = 0; s <= seg; s++) {
          const th = (s / seg) * TAU;
          const env = 0.5 + 0.5 * Math.sin(rb.k3 * th + rb.p * 2 + t * rb.s3);
          const rr =
            R *
            breathe *
            (1 +
              rb.off +
              rb.width * (u - 0.5) * (0.25 + 0.75 * env) +
              rb.a1 * Math.sin(rb.k1 * th + rb.p + t * rb.s1 + u * rb.d1) +
              rb.a2 * Math.sin(rb.k2 * th - rb.p + t * rb.s2 + u * rb.d2));
          const x = cx + rr * Math.cos(th + base);
          const y = cy + rr * Math.sin(th + base);
          if (s === 0) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        }
        ctx.stroke();
      }
    }

    ctx.fillStyle = '#ede9fe';
    for (const sk of sparks) {
      const a = sk.a + t * sk.sp;
      const rr = R * (1 + sk.r + 0.03 * Math.sin(t * 0.4 + sk.ph));
      const x = cx + rr * Math.cos(a);
      const y = cy + rr * Math.sin(a);
      if (x < -10 || y < -10 || x > w + 10 || y > h + 10) continue;
      const tw = 0.5 + 0.5 * Math.sin(t * sk.tw + sk.ph);
      ctx.globalAlpha = 0.12 * tw;
      ctx.beginPath();
      ctx.arc(x, y, sk.size * 3.2, 0, TAU);
      ctx.fill();
      ctx.globalAlpha = 0.35 + 0.65 * tw;
      ctx.beginPath();
      ctx.arc(x, y, sk.size, 0, TAU);
      ctx.fill();
    }
    ctx.globalAlpha = 1;

    // haze: a low-res, blurred copy of the silk
    fctx.clearRect(0, 0, fog.width, fog.height);
    if (nativeBlur) fctx.filter = `blur(${(fog.width * 0.014).toFixed(1)}px)`;
    fctx.drawImage(silk, 0, 0, fog.width, fog.height);
  };

  const paused = (): boolean => html.dataset.axMotion === 'paused' || document.hidden;

  const loop = (now: number): void => {
    const raw = (now - last) / 1000;
    last = now;
    clock += Math.min(0.1, raw);

    // Measure real frame time and step down a quality tier when it stays slow.
    frames++;
    if (frames > 40 && raw < 0.25) {
      slowAcc += raw;
      if (++slowN >= 45) {
        if (slowAcc / slowN > 0.023 && tier < TIERS.length - 1) {
          tier++;
          resize();
        }
        slowAcc = 0;
        slowN = 0;
      }
    }

    render(clock);
    raf = paused() ? 0 : requestAnimationFrame(loop);
  };

  const sync = (): void => {
    if (paused()) {
      cancelAnimationFrame(raf);
      raf = 0;
      render(clock);
    } else if (!raf) {
      last = performance.now();
      raf = requestAnimationFrame(loop);
    }
  };

  const onResize = (): void => {
    resize();
    if (!raf) render(clock);
  };

  resize();
  render(clock);
  sync();
  const observer = new ResizeObserver(onResize);
  observer.observe(root);
  window.addEventListener('ax:motion', sync);
  document.addEventListener('visibilitychange', sync);

  return () => {
    cancelAnimationFrame(raf);
    raf = 0;
    observer.disconnect();
    window.removeEventListener('ax:motion', sync);
    document.removeEventListener('visibilitychange', sync);
  };
}
