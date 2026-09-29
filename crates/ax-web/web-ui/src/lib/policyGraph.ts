export type PolicyGraphKind = 'rule' | 'skill' | 'memory';

export interface PolicyGraphPayload {
  nodes: {
    key: string;
    kind: PolicyGraphKind;
    id: string;
    label: string;
    origin: 'project' | 'global';
    projectId?: number;
  }[];
  edges: { source: string; target: string }[];
}

export interface PolicyGraphNode {
  key: string;
  kind: PolicyGraphKind;
  id: string;
  label: string;
  origin: 'project' | 'global';
  projectId?: number;
  global: boolean;
  degree: number;
}

export interface PolicyGraphModel {
  nodes: PolicyGraphNode[];
  edges: { source: string; target: string }[];
}

export function policyGraphModel(payload: PolicyGraphPayload | null): PolicyGraphModel {
  if (!payload) return { nodes: [], edges: [] };
  const keys = new Set(payload.nodes.map((n) => n.key));
  const edges = payload.edges.filter((e) => keys.has(e.source) && keys.has(e.target));
  const degree = new Map<string, number>();
  for (const e of edges) {
    degree.set(e.source, (degree.get(e.source) ?? 0) + 1);
    degree.set(e.target, (degree.get(e.target) ?? 0) + 1);
  }
  const nodes = payload.nodes.map((n) => ({
    ...n,
    global: n.origin === 'global',
    degree: degree.get(n.key) ?? 0,
  }));
  return { nodes, edges };
}

function linked(model: PolicyGraphModel, keys: string[]): PolicyGraphNode[] {
  const byKey = new Map(model.nodes.map((n) => [n.key, n]));
  return keys.flatMap((k) => byKey.get(k) ?? []);
}

export function outgoing(model: PolicyGraphModel, key: string): PolicyGraphNode[] {
  return linked(model, model.edges.filter((e) => e.source === key).map((e) => e.target));
}

export function backlinks(model: PolicyGraphModel, key: string): PolicyGraphNode[] {
  return linked(model, model.edges.filter((e) => e.target === key).map((e) => e.source));
}
