import { useEffect, useRef } from 'react';

/**
 * Mint CSS waves for Command Center chrome.
 * Fills the titlebar (see --titlebar-wave-h). Softness = SVG feGaussianBlur
 * (not CSS filter — that freezes transforms on many mobile WebViews).
 * Narrow / coarse pointers: JS translate3d pan (CSS SVG animation is unreliable).
 *
 * Uses both href and xlinkHref for older mobile WebViews.
 */
type Variant = 'titlebar' | 'panel' | 'workspace';

function prefersJsPan(): boolean {
  if (typeof window === 'undefined') return false;
  return (
    window.matchMedia('(max-width: 899px)').matches ||
    window.matchMedia('(pointer: coarse)').matches
  );
}

export default function HeaderWaves({ variant = 'titlebar' }: { variant?: Variant }) {
  const uid =
    variant === 'workspace'
      ? 'cc-gentle-wave-workspace'
      : variant === 'panel'
        ? 'cc-gentle-wave-panel'
        : 'cc-gentle-wave';
  const blurId = `${uid}-blur`;
  const href = `#${uid}`;
  const motionRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = motionRef.current;
    if (!el || !prefersJsPan()) return;

    el.style.animation = 'none';
    el.classList.add('cc-waves__motion--js');

    let raf = 0;
    const start = performance.now();
    // One-way duration; ping-pong for gentle continuous drift
    const halfMs = 7500;
    const ampPx = () => Math.max(48, el.offsetWidth * 0.07);

    const tick = (now: number) => {
      const cycle = ((now - start) % (halfMs * 2)) / halfMs;
      const t = cycle <= 1 ? cycle : 2 - cycle;
      // ease-in-out-ish without CSS
      const eased = t * t * (3 - 2 * t);
      const x = (eased * 2 - 1) * ampPx();
      el.style.transform = `translate3d(${x.toFixed(2)}px, 0, 0)`;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);

    const onResize = () => {
      /* ampPx reads live width each frame */
    };
    window.addEventListener('resize', onResize);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener('resize', onResize);
      el.classList.remove('cc-waves__motion--js');
      el.style.animation = '';
      el.style.transform = '';
    };
  }, []);

  return (
    <div className={`cc-waves cc-waves--${variant}`} aria-hidden="true">
      <div className="cc-waves__motion" ref={motionRef}>
        <svg
          className="cc-waves__svg"
          xmlns="http://www.w3.org/2000/svg"
          xmlnsXlink="http://www.w3.org/1999/xlink"
          viewBox="0 24 150 22"
          preserveAspectRatio="none"
        >
          <defs>
            <filter
              id={blurId}
              x="-25%"
              y="-50%"
              width="150%"
              height="200%"
              colorInterpolationFilters="sRGB"
            >
              <feGaussianBlur in="SourceGraphic" stdDeviation="4.5" />
            </filter>
            <path
              id={uid}
              d="M-160 44c30 0 58-18 88-18s 58 18 88 18 58-18 88-18 58 18 88 18 v44h-352z"
            />
          </defs>
          {/* translate(0,8) = tikkeltje naar beneden; filter = fuzzy edges */}
          <g className="cc-waves__parallax" filter={`url(#${blurId})`} transform="translate(0 8)">
            <use href={href} xlinkHref={href} x="48" y="-2" />
            <use href={href} xlinkHref={href} x="48" y="3" />
            <use href={href} xlinkHref={href} x="48" y="8" />
          </g>
        </svg>
      </div>
    </div>
  );
}
