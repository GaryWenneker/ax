export type HierarchyPlan = { planId: string; latestVersion: number };
export type HierarchySpec = { specId: string; plans: HierarchyPlan[] };

export type ParsedSpecs =
  | { mode: 'legacy'; specs: unknown[] }
  | { mode: 'hierarchy'; projectId: string; nodes: HierarchySpec[] };

export function parseSpecPayload(data: unknown): ParsedSpecs {
  if (!data || typeof data !== 'object') return { mode: 'legacy', specs: [] };
  const record = data as { mode?: unknown; projectId?: unknown; specs?: unknown };
  if (record.mode === 'hierarchy' && Array.isArray(record.specs)) {
    return {
      mode: 'hierarchy',
      projectId: typeof record.projectId === 'string' ? record.projectId : '',
      nodes: record.specs.flatMap(parseNode),
    };
  }
  return { mode: 'legacy', specs: Array.isArray(record.specs) ? record.specs : [] };
}

function parseNode(value: unknown): HierarchySpec[] {
  if (!value || typeof value !== 'object') return [];
  const spec = value as { specId?: unknown; plans?: unknown };
  if (typeof spec.specId !== 'string' || spec.specId.length === 0) return [];
  const plans = Array.isArray(spec.plans) ? spec.plans.flatMap(parsePlan) : [];
  return [{ specId: spec.specId, plans }];
}

function parsePlan(value: unknown): HierarchyPlan[] {
  if (!value || typeof value !== 'object') return [];
  const plan = value as { planId?: unknown; latestVersion?: unknown };
  if (typeof plan.planId !== 'string' || plan.planId.length === 0) return [];
  const version = typeof plan.latestVersion === 'number' ? plan.latestVersion : 0;
  return [{ planId: plan.planId, latestVersion: version }];
}

export type HierarchyRow = {
  key: string;
  specId: string;
  title: string;
  kind: 'spec' | 'plan';
  version: number | null;
};

export function hierarchyRows(nodes: HierarchySpec[]): HierarchyRow[] {
  const rows: HierarchyRow[] = [];
  for (const node of nodes) {
    rows.push({
      key: node.specId,
      specId: node.specId,
      title: node.specId,
      kind: 'spec',
      version: null,
    });
    for (const plan of node.plans) {
      rows.push({
        key: `${node.specId}/${plan.planId}`,
        specId: node.specId,
        title: plan.planId,
        kind: 'plan',
        version: plan.latestVersion,
      });
    }
  }
  return rows;
}

/** List rows for hierarchy mode: specs only; plans open under the blade. */
export function hierarchySpecRows(nodes: HierarchySpec[]): HierarchyRow[] {
  return nodes.map((node) => ({
    key: node.specId,
    specId: node.specId,
    title: node.specId,
    kind: 'spec' as const,
    version: null,
  }));
}

export function plansForSpec(nodes: HierarchySpec[], specId: string): HierarchyPlan[] {
  const node = nodes.find((entry) => entry.specId === specId);
  return node ? node.plans.slice() : [];
}
