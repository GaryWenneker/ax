import { useCallback, useRef, useState, type ReactNode } from 'react';

/**
 * Small "i" badge that reveals an explanatory tooltip on hover / focus / tap.
 * The tip uses position:fixed so it is never clipped by card or table
 * overflow containers.
 */
export function InfoHover({ label = 'More info', children }: { label?: string; children: ReactNode }) {
  const ref = useRef<HTMLButtonElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  const show = useCallback(() => {
    const r = ref.current?.getBoundingClientRect();
    if (!r) return;
    const half = 170; // half of max tip width + margin
    const left = Math.min(Math.max(r.left + r.width / 2, half), window.innerWidth - half);
    setPos({ top: r.bottom + 8, left });
  }, []);
  const hide = useCallback(() => setPos(null), []);

  return (
    <span className="info-hover">
      <button
        ref={ref}
        type="button"
        className="info-hover-trigger"
        aria-label={label}
        onMouseEnter={show}
        onMouseLeave={hide}
        onFocus={show}
        onBlur={hide}
        onClick={(e) => {
          e.preventDefault();
          if (pos) hide();
          else show();
        }}
      >
        i
      </button>
      {pos && (
        <span className="info-hover-tip" role="tooltip" style={{ top: pos.top, left: pos.left }}>
          {children}
        </span>
      )}
    </span>
  );
}
