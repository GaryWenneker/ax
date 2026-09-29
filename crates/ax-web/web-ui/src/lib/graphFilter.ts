import type { GraphGroup } from './graphSettings.ts';

export interface FilterNode {
  name: string;
  kind: string;
  file_path: string;
}

export interface FilterEdge {
  source: number;
  target: number;
}

export interface FilterOptions {
  search: string;
  orphans: boolean;
  attachments: boolean;
  existingOnly: boolean;
}

export interface FilterResult {
  nodes: Set<number>;
  edges: number[];
}

function matches(node: FilterNode, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return false;
  return node.name.toLowerCase().includes(q) || node.file_path.toLowerCase().includes(q);
}

export function filterGraph(nodes: FilterNode[], edges: FilterEdge[], opts: FilterOptions): FilterResult {
  const kept = new Set<number>();
  nodes.forEach((n, i) => {
    if (opts.search.trim() && !matches(n, opts.search)) return;
    if (!opts.attachments && n.kind === 'doc') return;
    if (opts.existingOnly && !n.file_path) return;
    kept.add(i);
  });
  const keptEdges: number[] = [];
  const linked = new Set<number>();
  edges.forEach((e, i) => {
    if (!kept.has(e.source) || !kept.has(e.target)) return;
    keptEdges.push(i);
    linked.add(e.source);
    linked.add(e.target);
  });
  if (!opts.orphans) {
    for (const i of [...kept]) if (!linked.has(i)) kept.delete(i);
  }
  return { nodes: kept, edges: keptEdges };
}

export function groupColor(node: FilterNode, groups: GraphGroup[]): string | null {
  for (const g of groups) if (matches(node, g.query)) return g.color;
  return null;
}
