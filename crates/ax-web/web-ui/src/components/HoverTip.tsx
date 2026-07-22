import { useCallback, useRef, useState, type CSSProperties, type ReactNode } from 'react';

/**
 * Wraps any element and shows a fixed-position tip on hover / focus.
 * Uses position:fixed so parent overflow never clips the tip.
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
  const [pos, setPos] = useState<{ top: number; left: number; place: 'above' | 'below' } | null>(null);

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
      tabIndex={0}
    >
      {children}
      {pos && (
        <span
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
