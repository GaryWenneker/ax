/** A simulation node that can be pinned in place while it is dragged. */
export interface Pinnable {
  fx?: number | null;
  fy?: number | null;
}

/** Pins node `i` at (x, y). Returns false when the node is gone (the graph shrank mid-drag). */
export function pinNode(nodes: Pinnable[], i: number, x: number, y: number): boolean {
  const n = nodes[i];
  if (!n) return false;
  n.fx = x;
  n.fy = y;
  return true;
}

/** Releases the pin on node `i`. Returns false when the node is gone. */
export function releaseNode(nodes: Pinnable[], i: number): boolean {
  const n = nodes[i];
  if (!n) return false;
  n.fx = null;
  n.fy = null;
  return true;
}
