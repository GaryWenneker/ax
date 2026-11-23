import type { HierarchySpec } from './hierarchyList';

export type LegacySpecItem = {
  id: string;
  title: string;
  path: string;
  kind: string;
  sessions: string;
  approvedVersion: number | null;
  latestStatus: string;
  latestVersion?: number | null;
  latestSource?: string | null;
  revisionCount: number;
  updatedAt?: number | null;
  /** Plans stored under this spec, oldest first. */
  plans?: { id: string; title: string; latestVersion: number; latestStatus: string }[];
};

export type WorkspaceStatusFilter = 'all' | 'draft' | 'approved' | 'change_requested';
/** Only `unclassified` can be derived today; phase and alternative need a stored relation. */
export type PlanRelation = 'phase' | 'alternative' | 'unclassified';

export type WorkspaceDoc = {
  id: string;
  type: 'spec' | 'plan';
  title: string;
  path: string;
  status: string;
  latestVersion: number | null;
  approvedVersion: number | null;
  source: string;
  updatedAt: number | null;
  sessions: string[];
  revisionCount: number | null;
  /** Set only when the parent is stored, never guessed. */
  parentSpecId: string | null;
  relation: PlanRelation | null;
};

export type WorkspaceSpec = { doc: WorkspaceDoc; plans: WorkspaceDoc[] };
export type Workspace = { specs: WorkspaceSpec[]; needsReview: WorkspaceDoc[]; all: WorkspaceDoc[] };

function byTitle(a: WorkspaceDoc, b: WorkspaceDoc): number {
  return a.title.localeCompare(b.title, undefined, { sensitivity: 'base' }) || a.id.localeCompare(b.id);
}

function newestFirst(a: WorkspaceDoc, b: WorkspaceDoc): number {
  return (b.updatedAt ?? 0) - (a.updatedAt ?? 0) || byTitle(a, b);
}

function flatten(specs: WorkspaceSpec[], needsReview: WorkspaceDoc[]): WorkspaceDoc[] {
  return [...specs.flatMap((spec) => [spec.doc, ...spec.plans]), ...needsReview];
}

function fromLegacyItem(item: LegacySpecItem): WorkspaceDoc {
  const type = item.kind === 'plan' ? 'plan' : 'spec';
  return {
    id: item.id,
    type,
    title: item.title || item.id,
    path: item.path || '',
    status: item.latestStatus || 'unknown',
    latestVersion: item.latestVersion ?? null,
    approvedVersion: item.approvedVersion ?? null,
    source: item.latestSource ?? '',
    updatedAt: item.updatedAt ?? null,
    sessions: (item.sessions || '').split(/\s+/).filter(Boolean),
    revisionCount: item.revisionCount ?? null,
    parentSpecId: null,
    relation: type === 'plan' ? 'unclassified' : null,
  };
}

function storedPlans(item: LegacySpecItem): WorkspaceDoc[] {
  return (item.plans ?? []).map((plan) => ({
    id: plan.id,
    type: 'plan',
    title: plan.title || plan.id,
    path: '',
    status: plan.latestStatus || 'unknown',
    latestVersion: plan.latestVersion,
    approvedVersion: null,
    source: '',
    updatedAt: null,
    sessions: [],
    revisionCount: null,
    parentSpecId: item.id,
    relation: 'unclassified',
  }));
}

export function workspaceFromLegacy(items: LegacySpecItem[]): Workspace {
  const specs = items
    .map((item) => ({ item, doc: fromLegacyItem(item) }))
    .filter(({ doc }) => doc.type === 'spec')
    .sort((a, b) => newestFirst(a.doc, b.doc))
    .map(({ item, doc }) => ({ doc, plans: storedPlans(item) }));
  const needsReview = items.map(fromLegacyItem).filter((doc) => doc.type === 'plan').sort(newestFirst);
  return { specs, needsReview, all: flatten(specs, needsReview) };
}

function hierarchyDoc(id: string, type: 'spec' | 'plan', latestVersion: number | null, parentSpecId: string | null): WorkspaceDoc {
  return {
    id,
    type,
    title: id,
    path: '',
    status: 'unknown',
    latestVersion,
    approvedVersion: null,
    source: '',
    updatedAt: null,
    sessions: [],
    revisionCount: null,
    parentSpecId,
    relation: type === 'plan' ? 'unclassified' : null,
  };
}

export function workspaceFromHierarchy(nodes: HierarchySpec[]): Workspace {
  const specs = nodes
    .map((node) => ({
      doc: hierarchyDoc(node.specId, 'spec', null, null),
      plans: node.plans.map((plan) => hierarchyDoc(plan.planId, 'plan', plan.latestVersion, node.specId)),
    }))
    .sort((a, b) => byTitle(a.doc, b.doc));
  return { specs, needsReview: [], all: flatten(specs, []) };
}

function matches(doc: WorkspaceDoc, needle: string, status: WorkspaceStatusFilter): boolean {
  if (status !== 'all' && doc.status !== status) return false;
  if (!needle) return true;
  return `${doc.title} ${doc.id} ${doc.path}`.toLowerCase().includes(needle);
}

export function filterWorkspace(ws: Workspace, query: string, status: WorkspaceStatusFilter): Workspace {
  const needle = query.trim().toLowerCase();
  const specs: WorkspaceSpec[] = [];
  for (const spec of ws.specs) {
    if (matches(spec.doc, needle, status)) {
      specs.push(needle ? spec : { doc: spec.doc, plans: spec.plans.filter((plan) => matches(plan, '', status)) });
      continue;
    }
    const plans = spec.plans.filter((plan) => matches(plan, needle, status));
    if (plans.length > 0) specs.push({ doc: spec.doc, plans });
  }
  const needsReview = ws.needsReview.filter((doc) => matches(doc, needle, status));
  const all = ws.all.filter((doc) => matches(doc, needle, status));
  return { specs, needsReview, all };
}

const CHECKLIST_LINE = /^\s*[-*+]\s+\[[ xX]\]\s/;

export function checklistLineCount(markdown: string): number {
  return markdown.split('\n').filter((line) => CHECKLIST_LINE.test(line)).length;
}

/** The explorer folds open the spec of the document being opened. */
export function expandOnOpen(expanded: ReadonlySet<string>, doc: WorkspaceDoc): Set<string> {
  const next = new Set(expanded);
  const spec = doc.type === 'spec' ? doc.id : doc.parentSpecId;
  if (spec) next.add(spec);
  return next;
}
