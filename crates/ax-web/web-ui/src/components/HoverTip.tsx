import { useCallback, useId, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { FOCUSABLE_SELECTOR, withDescribedBy, withoutDescribedBy } from '../lib/hoverTip';
import { pinPopover } from '../lib/placePopover';

/**
 * Wraps any element and shows a fixed-position tip on hover / focus.
 * Uses position:fixed so parent overflow never clips the tip.
 * The wrapper takes a tab stop only when the child has none; an interactive
 * child keeps its own stop and points at the tip with aria-describedby.
 */
export function HoverTip({
  tip,
  children,
  className,
  style,
  prefer = 'above',
}: {
  tip: ReactNode;
  children: ReactNode;
  className?: string;
  style?: CSSProperties;
  prefer?: 'above' | 'below';
}) {
  const ref = useRef<HTMLSpanElement>(null);
  const tipRef = useRef<HTMLSpanElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number; place: 'above' | 'below' } | null>(null);
  const tipId = useId();
  const [ownFocus, setOwnFocus] = useState(true);
  useLayoutEffect(() => {
    const inner = ref.current?.querySelector<HTMLElement>(FOCUSABLE_SELECTOR) ?? null;
    setOwnFocus(inner == null);
    if (!inner) return;
    inner.setAttribute('aria-describedby', withDescribedBy(inner.getAttribute('aria-describedby'), tipId));
    return () => {
      const rest = withoutDescribedBy(inner.getAttribute('aria-describedby'), tipId);
      if (rest) inner.setAttribute('aria-describedby', rest);
      else inner.removeAttribute('aria-describedby');
    };
  }, [children, tipId]);
  useLayoutEffect(() => {
    if (!pos) return;
    const rect = ref.current?.getBoundingClientRect();
    const tip = tipRef.current;
    if (!rect || !tip) return;
    pinPopover(rect, tip, { prefer });
  }, [pos, prefer]);

  const show = useCallback(() => {
    const r = ref.current?.getBoundingClientRect();
    if (!r) return;
    const place: 'above' | 'below' =
      prefer === 'below' || r.top < 72 ? 'below' : 'above';
    const half = 140;
    const left = Math.min(Math.max(r.left + r.width / 2, half), window.innerWidth - half);
    const top = place === 'below' ? r.bottom + 8 : r.top - 8;
    setPos({ top, left, place });
  }, [prefer]);

  const hide = useCallback(() => setPos(null), []);

  return (
    <span
      ref={ref}
      className={`hover-tip-wrap${className ? ` ${className}` : ''}`}
      style={style}
      onMouseEnter={show}
      onMouseLeave={hide}
      onFocus={show}
      onBlur={hide}
      tabIndex={ownFocus ? 0 : undefined}
      aria-describedby={ownFocus ? tipId : undefined}
    >
      {children}
      {pos && (
        <span
          ref={tipRef}
          id={tipId}
          className={`hover-tip hover-tip--${pos.place}`}
          role="tooltip"
          style={{ top: pos.top, left: pos.left }}
        >
          {tip}
        </span>
      )}
    </span>
  );
}
