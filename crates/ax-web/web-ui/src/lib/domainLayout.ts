export type DomainKind = 'domain' | 'flow' | 'step';
export type DomainEdgeKind = 'contains_flow' | 'flow_step' | 'cross_domain';

export interface DomainOverlayNode {
  id: string;
  kind: DomainKind;
  name: string;
  summary?: string;
  codeNodeIds?: string[];
}

export interface DomainOverlayEdge {
  source: string;
  target: string;
  kind: DomainEdgeKind;
  order?: number;
}

export interface DomainOverlay {
  version: number;
  nodes: DomainOverlayNode[];
  edges: DomainOverlayEdge[];
}

export interface LaidOutDomainNode extends DomainOverlayNode {
  x: number;
  y: number;
  community_id: number;
}

export function emptyDomainOverlay(): DomainOverlay {
  return { version: 1, nodes: [], edges: [] };
}

const COL_X = { domain: 100, flow: 340, step: 580 } as const;
const ROW = 56;

/** Place domain → flow → step left-to-right, stacked by owning domain. */
export function layoutDomainGraph(graph: DomainOverlay): LaidOutDomainNode[] {
  const byId = new Map(graph.nodes.map((n) => [n.id, n]));
  const flowsOf = new Map<string, DomainOverlayNode[]>();
  const stepsOf = new Map<string, Array<DomainOverlayNode & { order: number }>>();

  for (const e of graph.edges) {
    const src = byId.get(e.source);
    const tgt = byId.get(e.target);
    if (!src || !tgt) continue;
    if (e.kind === 'contains_flow' && src.kind === 'domain' && tgt.kind === 'flow') {
      const list = flowsOf.get(src.id) ?? [];
      list.push(tgt);
      flowsOf.set(src.id, list);
    } else if (e.kind === 'flow_step' && src.kind === 'flow' && tgt.kind === 'step') {
      const list = stepsOf.get(src.id) ?? [];
      list.push({ ...tgt, order: e.order ?? 0 });
      stepsOf.set(src.id, list);
    }
  }

  for (const list of stepsOf.values()) {
    list.sort((a, b) => a.order - b.order);
  }

  const placed = new Set<string>();
  const out: LaidOutDomainNode[] = [];
  const domains = graph.nodes.filter((n) => n.kind === 'domain');
  let y = 80;

  domains.forEach((domain, di) => {
    const flows = flowsOf.get(domain.id) ?? [];
    out.push({ ...domain, x: COL_X.domain, y, community_id: di });
    placed.add(domain.id);

    let fy = y;
    if (flows.length === 0) {
      fy = y + ROW;
    }
    for (const flow of flows) {
      out.push({ ...flow, x: COL_X.flow, y: fy, community_id: di });
      placed.add(flow.id);
      const steps = stepsOf.get(flow.id) ?? [];
      steps.forEach((step, si) => {
        out.push({
          id: step.id,
          kind: step.kind,
          name: step.name,
          summary: step.summary,
          codeNodeIds: step.codeNodeIds,
          x: COL_X.step,
          y: fy + si * ROW,
          community_id: di,
        });
        placed.add(step.id);
      });
      fy += Math.max(ROW, (steps.length || 1) * ROW) + 12;
    }
    y = Math.max(y + 100, fy + 16);
  });

  let orphanY = y;
  for (const node of graph.nodes) {
    if (placed.has(node.id)) continue;
    const x =
      node.kind === 'domain' ? COL_X.domain : node.kind === 'flow' ? COL_X.flow : COL_X.step;
    out.push({ ...node, x, y: orphanY, community_id: Math.max(0, domains.length) });
    orphanY += ROW;
  }

  return out;
}
