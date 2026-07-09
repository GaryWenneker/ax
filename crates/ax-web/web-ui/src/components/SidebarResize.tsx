import { useCallback, useEffect, useRef } from 'react';

const SIDEBAR_KEY = 'ax-web-sidebar-w';
export const SIDEBAR_MIN = 160;
export const SIDEBAR_MAX = 420;
export const SIDEBAR_DEFAULT = 200;

export function loadSidebarWidth(): number {
  const raw = localStorage.getItem(SIDEBAR_KEY);
  const n = raw ? Number.parseInt(raw, 10) : SIDEBAR_DEFAULT;
  if (Number.isNaN(n)) return SIDEBAR_DEFAULT;
  return Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, n));
}

export function applySidebarWidth(px: number) {
  const clamped = Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, px));
  document.documentElement.style.setProperty('--sidebar-w', `${clamped}px`);
  localStorage.setItem(SIDEBAR_KEY, String(clamped));
  return clamped;
}

export function initSidebarWidth() {
  applySidebarWidth(loadSidebarWidth());
}

export default function SidebarResizeHandle() {
  const dragging = useRef(false);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;
    document.body.classList.add('sidebar-resizing');
  }, []);

  useEffect(() => {
    function onMove(e: MouseEvent) {
      if (!dragging.current) return;
      applySidebarWidth(e.clientX);
    }

    function onUp() {
      if (!dragging.current) return;
      dragging.current = false;
      document.body.classList.remove('sidebar-resizing');
    }

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      document.body.classList.remove('sidebar-resizing');
    };
  }, []);

  return (
    <div
      className="sidebar-resize-handle"
      role="separator"
      aria-orientation="vertical"
      aria-valuemin={SIDEBAR_MIN}
      aria-valuemax={SIDEBAR_MAX}
      aria-valuenow={loadSidebarWidth()}
      aria-label="Resize sidebar"
      title="Drag to resize sidebar · double-click to reset"
      onMouseDown={onMouseDown}
      onDoubleClick={() => applySidebarWidth(SIDEBAR_DEFAULT)}
    />
  );
}
