/**
 * Azure CSS waves for Command Center chrome.
 * Adapted from https://codepen.io/goodkatz/pen/LYPGxQz — crest hangs with a soft
 * fade (not a flat clip), colors match site bokeh orbs.
 *
 * Uses both href and xlinkHref for older mobile WebViews.
 */
type Variant = 'titlebar' | 'panel' | 'workspace';

export default function HeaderWaves({ variant = 'titlebar' }: { variant?: Variant }) {
  const uid =
    variant === 'workspace'
      ? 'cc-gentle-wave-workspace'
      : variant === 'panel'
        ? 'cc-gentle-wave-panel'
        : 'cc-gentle-wave';
  const href = `#${uid}`;
  return (
    <div className={`cc-waves cc-waves--${variant}`} aria-hidden="true">
      <svg
        className="cc-waves__svg"
        xmlns="http://www.w3.org/2000/svg"
        xmlnsXlink="http://www.w3.org/1999/xlink"
        viewBox="0 24 150 28"
        preserveAspectRatio="none"
        shapeRendering="auto"
      >
        <defs>
          <path
            id={uid}
            d="M-160 44c30 0 58-18 88-18s 58 18 88 18 58-18 88-18 58 18 88 18 v44h-352z"
          />
        </defs>
        <g className="cc-waves__parallax">
          <use href={href} xlinkHref={href} x="48" y="0" />
          <use href={href} xlinkHref={href} x="48" y="3" />
          <use href={href} xlinkHref={href} x="48" y="5" />
          <use href={href} xlinkHref={href} x="48" y="7" />
        </g>
      </svg>
    </div>
  );
}
