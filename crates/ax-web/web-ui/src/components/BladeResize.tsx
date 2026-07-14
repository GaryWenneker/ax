import { useCallback, useEffect, useRef, type ReactNode } from 'react';

const BLADE_KEY = 'ax-web-blade-w';
export const BLADE_MIN = 280;
export const BLADE_MAX = 1200;
export const BLADE_DEFAULT = 380;

export function loadBladeWidth(): number {
  const raw = localStorage.getItem(BLADE_KEY);
  const n = raw ? Number.parseInt(raw, 10) : BLADE_DEFAULT;
  if (Number.isNaN(n)) return BLADE_DEFAULT;
  return Math.min(BLADE_MAX, Math.max(BLADE_MIN, n));
}

export function applyBladeWidth(px: number) {
  const max = Math.min(BLADE_MAX, Math.floor(window.innerWidth * 0.85));
  const clamped = Math.min(max, Math.max(BLADE_MIN, px));
  document.documentElement.style.setProperty('--blade-w', `${clamped}px`);
  localStorage.setItem(BLADE_KEY, String(clamped));
  return clamped;
}

export function initBladeWidth() {
  applyBladeWidth(loadBladeWidth());
}

function BladeResizeHandle() {
  const dragging = useRef(false);
  const startX = useRef(0);
  const startW = useRef(BLADE_DEFAULT);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;
    startX.current = e.clientX;
    startW.current = loadBladeWidth();
    document.body.classList.add('blade-resizing');
  }, []);

  useEffect(() => {
    function onMove(e: MouseEvent) {
      if (!dragging.current) return;
      // Dragging the left edge left → wider blade.
      const delta = startX.current - e.clientX;
      applyBladeWidth(startW.current + delta);
    }

    function onUp() {
      if (!dragging.current) return;
      dragging.current = false;
      document.body.classList.remove('blade-resizing');
    }

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      document.body.classList.remove('blade-resizing');
    };
  }, []);

  return (
    <div
      className="blade-resize-handle"
      role="separator"
      aria-orientation="vertical"
      aria-valuemin={BLADE_MIN}
      aria-valuemax={BLADE_MAX}
      aria-valuenow={loadBladeWidth()}
      aria-label="Resize detail panel"
      title="Drag to resize panel · double-click to reset"
      onMouseDown={onMouseDown}
      onDoubleClick={() => applyBladeWidth(BLADE_DEFAULT)}
    />
  );
}

/** Wraps a detail blade with a draggable left-edge resize handle. */
export function ResizableBlade({
  children,
  className = '',
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={`resizable-blade${className ? ` ${className}` : ''}`}>
      <BladeResizeHandle />
      <div className="resizable-blade-content">{children}</div>
    </div>
  );
}
