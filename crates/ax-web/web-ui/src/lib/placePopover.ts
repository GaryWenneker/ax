import { useLayoutEffect, type RefObject } from 'react';

/** Keep a fixed popover inside the viewport. Opens on the side that has room. */

const MARGIN = 8;
const GAP = 6;

export function placePopover(
  anchor: DOMRect,
  size: { width: number; height: number },
  options?: { prefer?: 'below' | 'above'; align?: 'start' | 'end' },
): { top: number; left: number; maxHeight: number } {
  const prefer = options?.prefer ?? 'below';
  const align = options?.align ?? 'start';
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const width = Math.min(Math.max(size.width, 0), Math.max(0, vw - MARGIN * 2));
  let left = align === 'end' ? anchor.right - width : anchor.left;
  if (left + width > vw - MARGIN) left = vw - MARGIN - width;
  if (left < MARGIN) left = MARGIN;

  const spaceBelow = Math.max(0, vh - anchor.bottom - GAP - MARGIN);
  const spaceAbove = Math.max(0, anchor.top - GAP - MARGIN);
  const want = Math.max(size.height, 1);
  const openBelow = prefer === 'above'
    ? spaceAbove < Math.min(want, 120) && spaceBelow > spaceAbove
    : spaceBelow >= Math.min(want, 120) || spaceBelow >= spaceAbove;
  const available = Math.max(48, openBelow ? spaceBelow : spaceAbove);
  const height = Math.min(want, available);
  const top = openBelow
    ? Math.min(anchor.bottom + GAP, Math.max(MARGIN, vh - MARGIN - height))
    : Math.max(MARGIN, anchor.top - GAP - height);
  return { top, left, maxHeight: available };
}

/** Write fixed coordinates so an already-mounted popover stays on screen. */
export function pinPopover(
  anchor: DOMRect,
  element: HTMLElement,
  options?: { prefer?: 'below' | 'above'; align?: 'start' | 'end' },
): void {
  element.style.position = 'fixed';
  element.style.right = 'auto';
  element.style.bottom = 'auto';
  element.style.transform = 'none';
  if (!element.dataset.popoverMax) {
    const styled = getComputedStyle(element).maxWidth;
    element.dataset.popoverMax = Number.isFinite(Number.parseFloat(styled)) ? styled : 'none';
  }
  const viewportCap = Math.max(0, window.innerWidth - MARGIN * 2);
  const styledCap = element.dataset.popoverMax === 'none' ? NaN : Number.parseFloat(element.dataset.popoverMax);
  element.style.maxWidth = `${Number.isFinite(styledCap) ? Math.min(styledCap, viewportCap) : viewportCap}px`;
  const box = placePopover(anchor, { width: element.offsetWidth, height: element.scrollHeight }, options);
  element.style.top = `${box.top}px`;
  element.style.left = `${box.left}px`;
  element.style.maxHeight = `${box.maxHeight}px`;
  element.style.overflow = 'auto';
  element.style.zIndex = '4000';
}

/** Re-pin while `open`. Scroll and resize keep the popover inside the viewport. */
export function usePinnedPopover(
  open: boolean,
  anchor: RefObject<HTMLElement | null>,
  panel: RefObject<HTMLElement | null>,
  options?: { prefer?: 'below' | 'above'; align?: 'start' | 'end'; watch?: unknown },
): void {
  const prefer = options?.prefer ?? 'below';
  const align = options?.align ?? 'start';
  const watch = options?.watch;
  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      const rect = anchor.current?.getBoundingClientRect();
      const el = panel.current;
      if (!rect || !el || el.closest('dialog, .ax-modal')) return;
      pinPopover(rect, el, { prefer, align });
    };
    place();
    window.addEventListener('resize', place);
    window.addEventListener('scroll', place, true);
    return () => {
      window.removeEventListener('resize', place);
      window.removeEventListener('scroll', place, true);
    };
  }, [open, anchor, panel, prefer, align, watch]);
}
