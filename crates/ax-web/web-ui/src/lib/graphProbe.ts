/** Where one graph node is drawn, in viewport (client) pixels. */
export interface ScreenNode {
  key: string;
  x: number;
  y: number;
}

type ProbeElement = HTMLElement & { axGraphNodes?: () => ScreenNode[] };

/**
 * Let browser tests find nodes on a canvas graph. `data-settled` on the wrapper says
 * whether the layout has come to rest, and `element.axGraphNodes()` returns where
 * each visible node is drawn right now, so a test can wait for the layout and point
 * at a node instead of sleeping and scanning pixels.
 */
export function exposeGraphNodes(wrap: HTMLElement | null, nodes: () => ScreenNode[]): void {
  if (wrap) (wrap as ProbeElement).axGraphNodes = nodes;
}

export function markGraphSettled(wrap: HTMLElement | null, settled: boolean): void {
  if (wrap) wrap.dataset.settled = settled ? 'true' : 'false';
}

/** Nodes whose centre lies inside `rect`, translated from world to client pixels. */
export function screenNodes<T>(
  items: Iterable<T>,
  place: (item: T) => { key: string; x: number; y: number } | null,
  transform: { scale: number; offsetX: number; offsetY: number },
  rect: { left: number; top: number; width: number; height: number },
): ScreenNode[] {
  const out: ScreenNode[] = [];
  for (const item of items) {
    const p = place(item);
    if (!p || !Number.isFinite(p.x) || !Number.isFinite(p.y)) continue;
    const cx = transform.offsetX + p.x * transform.scale;
    const cy = transform.offsetY + p.y * transform.scale;
    if (cx < 0 || cy < 0 || cx > rect.width || cy > rect.height) continue;
    out.push({ key: p.key, x: rect.left + cx, y: rect.top + cy });
  }
  return out;
}
