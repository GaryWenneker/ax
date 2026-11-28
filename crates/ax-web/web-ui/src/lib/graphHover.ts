import type { FilterEdge } from './graphFilter.ts';

export interface Incidence {
  node: number;
  edge: number;
}

export type NeighborIndex = Incidence[][];

export interface Highlight {
  nodes: Set<number>;
  edges: Set<number>;
}

export const PULSE_MS = 1600;

/** Selection ring: `grow` 0..1 outward from the node, `alpha` fades as it grows; loops every PULSE_MS. */
export function selectionPulse(timeMs: number): { grow: number; alpha: number } {
  const t = (((timeMs % PULSE_MS) + PULSE_MS) % PULSE_MS) / PULSE_MS;
  const grow = 1 - (1 - t) * (1 - t);
  return { grow, alpha: 0.9 * (1 - t) };
}

/** Full pulses the selection ring plays before it holds still. */
export const SELECTION_PULSE_CYCLES = 3;

/** True once the selection ring has finished pulsing `elapsedMs` after the selection. */
export function selectionPulseDone(elapsedMs: number): boolean {
  return elapsedMs >= PULSE_MS * SELECTION_PULSE_CYCLES;
}

/** The selection ring `elapsedMs` after the selection: a few pulses, then a steady ring. */
export function selectionRing(elapsedMs: number): { grow: number; alpha: number } {
  return selectionPulseDone(elapsedMs) ? { grow: 0, alpha: 0.9 } : selectionPulse(elapsedMs);
}

export function buildNeighbors(nodeCount: number, edges: FilterEdge[]): NeighborIndex {
  const index: NeighborIndex = Array.from({ length: nodeCount }, () => []);
  edges.forEach((e, i) => {
    index[e.source]?.push({ node: e.target, edge: i });
    index[e.target]?.push({ node: e.source, edge: i });
  });
  return index;
}

export function highlightFor(hovered: number | null, index: NeighborIndex): Highlight {
  const nodes = new Set<number>();
  const edges = new Set<number>();
  if (hovered == null) return { nodes, edges };
  nodes.add(hovered);
  for (const inc of index[hovered] ?? []) {
    nodes.add(inc.node);
    edges.add(inc.edge);
  }
  return { nodes, edges };
}
