import type { GraphSettings } from './graphSettings.ts';

export interface ForceParams {
  charge: number;
  center: number;
  linkStrength: number;
  linkDistance: number;
}

const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));

/** Sunflower spiral: deterministic, evenly filled disc of radius r. */
export function seedCluster(n: number, cx: number, cy: number, r: number): { x: number; y: number }[] {
  const out: { x: number; y: number }[] = [];
  for (let i = 0; i < n; i++) {
    const d = r * Math.sqrt((i + 0.5) / Math.max(1, n));
    const a = i * GOLDEN_ANGLE;
    out.push({ x: cx + d * Math.cos(a), y: cy + d * Math.sin(a) });
  }
  return out;
}

export function forceParams(s: GraphSettings): ForceParams {
  return {
    charge: -8 * s.repelForce,
    center: s.centerForce * 0.2,
    linkStrength: s.linkForce,
    linkDistance: s.linkDistance,
  };
}

/** Changes exactly when a setting that moves nodes changes (forces and the collision radius). */
export function simulationKey(s: GraphSettings): string {
  const f = forceParams(s);
  return [f.charge, f.center, f.linkStrength, f.linkDistance, s.nodeSize].join('|');
}

/** Changes exactly when a setting that decides which nodes are shown changes. */
export function filterKey(s: GraphSettings): string {
  return JSON.stringify([s.search, s.attachments, s.existingOnly, s.orphans]);
}
