import { useCallback, useEffect, useRef } from 'react';

const META_KEY = 'ax-web-policy-meta-w';
export const POLICY_META_MIN = 220;
export const POLICY_META_MAX = 520;
export const POLICY_META_DEFAULT = 300;

const MD_SPLIT_KEY = 'ax-web-md-edit-pct';
export const MD_EDIT_PCT_MIN = 28;
export const MD_EDIT_PCT_MAX = 72;
export const MD_EDIT_PCT_DEFAULT = 50;

const LIST_KEY = 'ax-web-policy-list-w';
export const POLICY_LIST_MIN = 140;
export const POLICY_LIST_MAX = 480;
export const POLICY_LIST_DEFAULT = 200;

export function loadPolicyMetaWidth(): number {
  const raw = localStorage.getItem(META_KEY);
  const n = raw ? Number.parseInt(raw, 10) : POLICY_META_DEFAULT;
  if (Number.isNaN(n)) return POLICY_META_DEFAULT;
  return Math.min(POLICY_META_MAX, Math.max(POLICY_META_MIN, n));
}

export function applyPolicyMetaWidth(px: number) {
  const clamped = Math.min(POLICY_META_MAX, Math.max(POLICY_META_MIN, px));
  document.documentElement.style.setProperty('--policy-meta-w', `${clamped}px`);
  localStorage.setItem(META_KEY, String(clamped));
  return clamped;
}

export function initPolicyMetaWidth() {
  applyPolicyMetaWidth(loadPolicyMetaWidth());
}

export function loadMdEditPct(): number {
  const raw = localStorage.getItem(MD_SPLIT_KEY);
  const n = raw ? Number.parseFloat(raw) : MD_EDIT_PCT_DEFAULT;
  if (Number.isNaN(n)) return MD_EDIT_PCT_DEFAULT;
  return Math.min(MD_EDIT_PCT_MAX, Math.max(MD_EDIT_PCT_MIN, n));
}

export function applyMdEditPct(pct: number, el?: HTMLElement | null) {
  const clamped = Math.min(MD_EDIT_PCT_MAX, Math.max(MD_EDIT_PCT_MIN, pct));
  const target = el ?? document.documentElement;
  target.style.setProperty('--md-edit-pct', `${clamped}%`);
  localStorage.setItem(MD_SPLIT_KEY, String(clamped));
  return clamped;
}

export function loadPolicyListWidth(): number {
  const raw = localStorage.getItem(LIST_KEY);
  const n = raw ? Number.parseInt(raw, 10) : POLICY_LIST_DEFAULT;
  if (Number.isNaN(n)) return POLICY_LIST_DEFAULT;
  return Math.min(POLICY_LIST_MAX, Math.max(POLICY_LIST_MIN, n));
}

export function applyPolicyListWidth(px: number) {
  const clamped = Math.min(POLICY_LIST_MAX, Math.max(POLICY_LIST_MIN, px));
  document.documentElement.style.setProperty('--policy-list-w', `${clamped}px`);
  localStorage.setItem(LIST_KEY, String(clamped));
  return clamped;
}

export function initPolicyListWidth() {
  applyPolicyListWidth(loadPolicyListWidth());
}

function ResizeGrip() {
  return <span className="policy-resize-grip" aria-hidden />;
}

/** Drag handle between rules/skills ID list and the detail workspace. */
export function PolicyListResizeHandle() {
  const dragging = useRef(false);
  const startX = useRef(0);
  const startW = useRef(POLICY_LIST_DEFAULT);

  useEffect(() => {
    initPolicyListWidth();
  }, []);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;
    startX.current = e.clientX;
    startW.current = loadPolicyListWidth();
    document.body.classList.add('policy-list-resizing');
  }, []);

  useEffect(() => {
    function onMove(e: MouseEvent) {
      if (!dragging.current) return;
      applyPolicyListWidth(startW.current + (e.clientX - startX.current));
    }

    function onUp() {
      if (!dragging.current) return;
      dragging.current = false;
      document.body.classList.remove('policy-list-resizing');
    }

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      document.body.classList.remove('policy-list-resizing');
    };
  }, []);

  return (
    <div
      className="policy-list-resize-handle"
      role="separator"
      aria-orientation="vertical"
      aria-valuemin={POLICY_LIST_MIN}
      aria-valuemax={POLICY_LIST_MAX}
      aria-valuenow={loadPolicyListWidth()}
      aria-label="Resize rules list"
      title="Drag to resize list · double-click to reset"
      onMouseDown={onMouseDown}
      onDoubleClick={() => applyPolicyListWidth(POLICY_LIST_DEFAULT)}
    >
      <ResizeGrip />
    </div>
  );
}

/** Drag handle between metadata column and markdown panel. */
export default function PolicyMetaResizeHandle() {
  const dragging = useRef(false);
  const startX = useRef(0);
  const startW = useRef(POLICY_META_DEFAULT);

  useEffect(() => {
    initPolicyMetaWidth();
  }, []);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;
    startX.current = e.clientX;
    startW.current = loadPolicyMetaWidth();
    document.body.classList.add('policy-meta-resizing');
  }, []);

  useEffect(() => {
    function onMove(e: MouseEvent) {
      if (!dragging.current) return;
      applyPolicyMetaWidth(startW.current + (e.clientX - startX.current));
    }

    function onUp() {
      if (!dragging.current) return;
      dragging.current = false;
      document.body.classList.remove('policy-meta-resizing');
    }

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      document.body.classList.remove('policy-meta-resizing');
    };
  }, []);

  return (
    <div
      className="policy-meta-resize-handle"
      role="separator"
      aria-orientation="vertical"
      aria-valuemin={POLICY_META_MIN}
      aria-valuemax={POLICY_META_MAX}
      aria-valuenow={loadPolicyMetaWidth()}
      aria-label="Resize metadata panel"
      title="Drag to resize metadata · double-click to reset"
      onMouseDown={onMouseDown}
      onDoubleClick={() => applyPolicyMetaWidth(POLICY_META_DEFAULT)}
    >
      <ResizeGrip />
    </div>
  );
}

/** Vertical drag handle between markdown source and live preview. */
export function MdPreviewResizeHandle({ containerRef }: { containerRef: React.RefObject<HTMLElement | null> }) {
  const dragging = useRef(false);
  const startX = useRef(0);
  const startPct = useRef(MD_EDIT_PCT_DEFAULT);

  useEffect(() => {
    applyMdEditPct(loadMdEditPct(), containerRef.current);
  }, [containerRef]);

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      dragging.current = true;
      startX.current = e.clientX;
      startPct.current = loadMdEditPct();
      document.body.classList.add('md-split-resizing');
    },
    [],
  );

  useEffect(() => {
    function onMove(e: MouseEvent) {
      if (!dragging.current) return;
      const el = containerRef.current;
      if (!el) return;
      const width = el.getBoundingClientRect().width;
      if (width <= 0) return;
      const deltaPct = ((e.clientX - startX.current) / width) * 100;
      applyMdEditPct(startPct.current + deltaPct, el);
    }

    function onUp() {
      if (!dragging.current) return;
      dragging.current = false;
      document.body.classList.remove('md-split-resizing');
    }

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      document.body.classList.remove('md-split-resizing');
    };
  }, [containerRef]);

  return (
    <div
      className="md-preview-resize-handle"
      role="separator"
      aria-orientation="vertical"
      aria-valuemin={MD_EDIT_PCT_MIN}
      aria-valuemax={MD_EDIT_PCT_MAX}
      aria-valuenow={loadMdEditPct()}
      aria-label="Resize editor and preview"
      title="Drag to resize source / preview · double-click to reset"
      onMouseDown={onMouseDown}
      onDoubleClick={() => applyMdEditPct(MD_EDIT_PCT_DEFAULT, containerRef.current)}
    >
      <ResizeGrip />
    </div>
  );
}
