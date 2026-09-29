export interface LabelBox {
  x: number;
  y: number;
  w: number;
  h: number;
  priority: number;
}

export const LABEL_MIN_PX = 11;
export const LABEL_MAX_PX = 15;

/** Screen font size: base size up to zoom 1, then gentle growth, hard-capped. */
export function labelPx(scale: number): number {
  const grown = LABEL_MIN_PX * Math.pow(Math.max(1, scale), 0.25);
  return Math.min(LABEL_MAX_PX, grown);
}

/** Greedy label placement: highest priority first, skip any label that overlaps a placed one. */
export function placeLabels(boxes: LabelBox[]): Set<number> {
  const order = boxes.map((_, i) => i).sort((a, b) => boxes[b].priority - boxes[a].priority || a - b);
  const placed: LabelBox[] = [];
  const kept = new Set<number>();
  for (const i of order) {
    const b = boxes[i];
    const hit = placed.some((p) => b.x < p.x + p.w && p.x < b.x + b.w && b.y < p.y + p.h && p.y < b.y + b.h);
    if (hit) continue;
    placed.push(b);
    kept.add(i);
  }
  return kept;
}

/** Label opacity ramps from 0 to 1 as zoom passes the fade threshold. */
export function labelAlpha(scale: number, threshold: number): number {
  const start = 1.8 + threshold * 0.4;
  return Math.max(0, Math.min(1, (scale - start) / 0.4));
}
