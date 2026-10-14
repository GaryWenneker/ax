import { useCallback, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { pinPopover } from '../../lib/placePopover';
import useMobile from '../../hooks/useMobile';
import ModalShell from '../ModalShell';

/**
 * Small "i" badge that reveals an explanatory tooltip on hover / focus / tap.
 * The tip uses position:fixed so it is never clipped by card or table
 * overflow containers.
 */
export function InfoHover({ label = 'More info', children }: { label?: string; children: ReactNode }) {
  const mobile = useMobile();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLButtonElement>(null);
  const tipRef = useRef<HTMLSpanElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  useLayoutEffect(() => {
    if (!pos) return;
    const rect = ref.current?.getBoundingClientRect();
    const tip = tipRef.current;
    if (!rect || !tip) return;
    pinPopover(rect, tip, { prefer: 'below', align: 'start' });
  }, [pos]);

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
        aria-haspopup={mobile ? 'dialog' : undefined}
        aria-expanded={mobile ? open : undefined}
        onMouseEnter={mobile ? undefined : show}
        onMouseLeave={hide}
        onFocus={mobile ? undefined : show}
        onBlur={hide}
        onClick={(e) => {
          e.preventDefault();
          if (mobile) {setOpen(true);return;}
          if (pos) hide();
          else show();
        }}
      >
        i
      </button>
      {mobile && open && <ModalShell title={label} onClose={() => setOpen(false)}>{children}</ModalShell>}
      {pos && (
        <span ref={tipRef} className="info-hover-tip" role="tooltip" style={{ top: pos.top, left: pos.left }}>
          {children}
        </span>
      )}
    </span>
  );
}
