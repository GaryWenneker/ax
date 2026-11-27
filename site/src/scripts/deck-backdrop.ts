/**
 * Site backdrop, ported from the Ax presentation deck: a breathing lime halftone field and a foggy
 * violet silk ring, on every page.
 *
 * - Silk ring: each layer is a bundle of fine closed curves around one ring. The radius is
 *   modulated by two travelling waves; a per-line phase shift makes the lines cross like woven
 *   silk, and an envelope pinches the bundle into folds. Layers add up (`lighter`), a blurred
 *   low-res copy gives the haze, and sparkles ride the layers.
 * - Halftone: lime dots that swell near the ring centre and ripple with slow interference waves.
 *
 * The homepage uses the `hero` variant (large ring right of the copy). Docs pages use `page`,
 * like the deck's inner slides: a wider, fainter ring whose position follows the page, so each
 * page has its own composition.
 *
 * Motion follows html[data-ax-motion] (the visible Pause motion control), prefers-reduced-motion
 * and hidden tabs: when paused it draws one still frame and stops the loop. The light docs theme
 * hides the backdrop, and then nothing is drawn.
 */

const TAU = Math.PI * 2;
const LIME = '#b9ff6a';

export type Variant = 'hero' | 'page';

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

/** Stable small number for a page path, so a page always gets the same composition. */
export function pageSeed(path: string): number {
  let hash = 0;
  for (const ch of path.replace(/\/+$/, '')) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return hash % 997;
}

export type Layout = {
  fx: number; // ring centre, fraction of width
  fy: number; // ring centre, fraction of height
  r: number; // ring radius, fraction of the short side
  k: number; // presence: 1 = hero, lower = quieter inner page
  spin: number; // extra rotation so pages do not repeat the same fold
};

/**
 * Where the ring sits. Hero: right of the copy on wide screens, up and off the edge on narrow ones.
 * Page: like the deck's inner slides, the centre walks around the screen by the golden angle per
 * page, but stays on the right half on wide screens so the article column stays calm.
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

// Pixel budgets per quality tier. A slow frame rate steps down a tier: fewer pixels, lines and segments.
const TIERS = [
  { px: 1.6e6, q: 1, seg: 150 },
  { px: 0.9e6, q: 0.7, seg: 120 },
  { px: 0.5e6, q: 0.5, seg: 96 },
  { px: 0.3e6, q: 0.36, seg: 80 },
] as const;

export function mountDeckBackdrop(root: HTMLElement): () => void {
  const dots = root.querySelector<HTMLCanvasElement>('[data-ax-backdrop-dots]');
  const silk = root.querySelector<HTMLCanvasElement>('[data-ax-silk]');
  const fog = root.querySelector<HTMLCanvasElement>('[data-ax-silk-fog]');
  const dctx = dots?.getContext('2d');
  const ctx = silk?.getContext('2d');
  const fctx = fog?.getContext('2d');
  if (!dots || !silk || !fog || !dctx || !ctx || !fctx) return () => {};

  const variant: Variant = root.dataset.axVariant === 'page' ? 'page' : 'hero';
  const seed = pageSeed(location.pathname);
  const sparks = makeSparks(variant === 'hero' ? 90 : 70);
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
    const ds = Math.min(dpr, Math.sqrt(1.6e6 / (w * h)));
    dots.width = Math.ceil(w * ds);
    dots.height = Math.ceil(h * ds);
    dctx.setTransform(ds, 0, 0, ds, 0, 0);
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
    const layout = backdropLayout(variant, w, h, seed);
    const m = Math.min(w, h);
    const R = m * layout.r;
    const cx = layout.fx * w;
    const cy = layout.fy * h;
    const strength = 0.75 + 0.25 * layout.k;

    // --- halftone field, centred on the ring ---
    dctx.clearRect(0, 0, w, h);
    const sp = Math.max(14, Math.round(w / 96));
    const aspect = w / h;
    dctx.fillStyle = LIME;
    dctx.globalAlpha = 0.09 + 0.2 * layout.k;
    dctx.beginPath();
    for (let y = sp / 2; y < h; y += sp) {
      for (let x = sp / 2; x < w; x += sp) {
        const dx = (x / w - layout.fx) * aspect;
        const dy = y / h - layout.fy;
        const d = Math.sqrt(dx * dx + dy * dy);
        const n =
          Math.sin(x * 0.011 + t * 0.6) +
          Math.sin(y * 0.014 - t * 0.45) +
          Math.sin((x + y) * 0.007 + t * 0.3);
        const v = 1 - d * 1.5 + n * 0.2;
        if (v <= 0.04) continue;
        const r = Math.min(v, 1) * sp * 0.4 * (0.55 + 0.45 * layout.k);
        dctx.moveTo(x + r, y);
        dctx.arc(x, y, r, 0, TAU);
      }
    }
    dctx.fill();

    // --- silk layers ---
    ctx.globalCompositeOperation = 'source-over';
    ctx.clearRect(0, 0, w, h);
    ctx.globalCompositeOperation = 'lighter';
    ctx.lineWidth = 1;

    for (const rb of RIBBONS) {
      ctx.strokeStyle = rb.color;
      const lines = Math.max(6, Math.round(rb.lines * q));
      // fewer lines on a lower tier, each a little brighter, so the ring keeps its weight
      ctx.globalAlpha = Math.min(1, (rb.alpha * strength * rb.lines) / lines);
      const base = rb.p + layout.spin * 0.6 + t * rb.rot;
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

    // --- sparkles ---
    ctx.fillStyle = '#ede9fe';
    for (const sk of sparks) {
      const a = sk.a + t * sk.sp + layout.spin * 0.6;
      const rr = R * (1 + sk.r + 0.03 * Math.sin(t * 0.4 + sk.ph));
      const x = cx + rr * Math.cos(a);
      const y = cy + rr * Math.sin(a);
      if (x < -10 || y < -10 || x > w + 10 || y > h + 10) continue;
      const tw = 0.5 + 0.5 * Math.sin(t * sk.tw + sk.ph);
      ctx.globalAlpha = 0.12 * tw * strength;
      ctx.beginPath();
      ctx.arc(x, y, sk.size * 3.2, 0, TAU);
      ctx.fill();
      ctx.globalAlpha = (0.35 + 0.65 * tw) * strength;
      ctx.beginPath();
      ctx.arc(x, y, sk.size, 0, TAU);
      ctx.fill();
    }
    ctx.globalAlpha = 1;

    // --- haze: a low-res, blurred copy of the silk ---
    fctx.clearRect(0, 0, fog.width, fog.height);
    if (nativeBlur) fctx.filter = `blur(${(fog.width * 0.014).toFixed(1)}px)`;
    fctx.drawImage(silk, 0, 0, fog.width, fog.height);
  };

  // Hidden by the light theme (display:none) → nothing to draw.
  const visible = (): boolean => root.getClientRects().length > 0;
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
    raf = paused() || !visible() ? 0 : requestAnimationFrame(loop);
  };

  const sync = (): void => {
    if (!visible()) {
      cancelAnimationFrame(raf);
      raf = 0;
    } else if (paused()) {
      cancelAnimationFrame(raf);
      raf = 0;
      render(clock);
    } else if (!raf) {
      last = performance.now();
      raf = requestAnimationFrame(loop);
    }
  };

  const onResize = (): void => {
    if (!visible()) return;
    resize();
    if (!raf) render(clock);
  };

  resize();
  if (visible()) render(clock);
  sync();
  const observer = new ResizeObserver(onResize);
  observer.observe(root);
  // The theme picker and the motion control both flip attributes on <html>.
  const attributes = new MutationObserver(() => {
    onResize();
    sync();
  });
  attributes.observe(html, { attributes: true, attributeFilter: ['data-theme', 'data-ax-motion'] });
  window.addEventListener('ax:motion', sync);
  document.addEventListener('visibilitychange', sync);

  return () => {
    cancelAnimationFrame(raf);
    raf = 0;
    observer.disconnect();
    attributes.disconnect();
    window.removeEventListener('ax:motion', sync);
    document.removeEventListener('visibilitychange', sync);
  };
}
