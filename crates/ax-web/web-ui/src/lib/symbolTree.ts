import type { NodeRow } from '../types';

export interface SymbolTreeNode {
  node: NodeRow;
  children: SymbolTreeNode[];
}

const KIND_ORDER: Record<string, number> = {
  file: 0,
  module: 1,
  namespace: 2,
  class: 3,
  struct: 4,
  interface: 5,
  enum: 6,
  trait: 7,
  type: 8,
  function: 9,
  method: 10,
  const: 11,
  variable: 12,
};

const TYPE_KINDS = new Set(['interface', 'type', 'class', 'struct', 'enum', 'trait']);

const KIND_SECTION_LABEL: Record<string, string> = {
  interface: 'Interfaces',
  type: 'Types',
  class: 'Classes',
  struct: 'Structs',
  enum: 'Enums',
  trait: 'Traits',
  function: 'Functions',
  method: 'Methods',
  const: 'Constants',
  variable: 'Variables',
  module: 'Modules',
  file: 'File',
};

function kindRank(kind: string): number {
  return KIND_ORDER[kind] ?? 50;
}

function compareNodes(a: NodeRow, b: NodeRow): number {
  const byKind = kindRank(a.kind) - kindRank(b.kind);
  if (byKind !== 0) return byKind;
  const byName = a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
  if (byName !== 0) return byName;
  return a.start_line - b.start_line;
}

function span(node: NodeRow): number {
  return Math.max(0, node.end_line - node.start_line);
}

function strictlyContains(outer: NodeRow, inner: NodeRow): boolean {
  if (outer.id === inner.id) return false;
  if (outer.start_line > inner.start_line) return false;
  if (outer.end_line < inner.end_line) return false;
  if (outer.start_line === inner.start_line && outer.end_line === inner.end_line) {
    return kindRank(outer.kind) < kindRank(inner.kind);
  }
  return true;
}

function findContainingParent(node: NodeRow, candidates: NodeRow[]): NodeRow | null {
  let best: NodeRow | null = null;
  let bestSpan = Number.POSITIVE_INFINITY;
  for (const candidate of candidates) {
    if (!strictlyContains(candidate, node)) continue;
    const s = span(candidate);
    if (s < bestSpan) {
      bestSpan = s;
      best = candidate;
    }
  }
  return best;
}

function findRelatedType(fn: NodeRow, types: NodeRow[]): NodeRow | null {
  const hay = `${fn.signature ?? ''} ${fn.qualified_name} ${fn.name}`;
  let best: NodeRow | null = null;
  let bestLen = 0;
  for (const t of types) {
    if (t.name.length < 3) continue;
    if (hay.includes(t.name) && t.name.length > bestLen) {
      best = t;
      bestLen = t.name.length;
    }
  }
  return best;
}

/** Build a sorted symbol tree: line containment + return-type / name association. */
export function buildSymbolTree(nodes: NodeRow[]): SymbolTreeNode[] {
  if (nodes.length === 0) return [];

  const fileNode = nodes.find((n) => n.kind === 'file') ?? null;
  const types = nodes.filter((n) => TYPE_KINDS.has(n.kind));
  const parentId = new Map<string, string | null>();

  for (const node of nodes) {
    if (node.kind === 'file') continue;

    let parent = findContainingParent(node, nodes);
    if (!parent && fileNode) {
      parent = fileNode;
    }

    if (
      parent?.kind === 'file' &&
      (node.kind === 'function' || node.kind === 'method')
    ) {
      const related = findRelatedType(node, types);
      if (related) {
        parent = related;
      }
    }

    parentId.set(node.id, parent?.id ?? null);
  }

  const childIds = new Map<string | null, string[]>();
  for (const node of nodes) {
    if (node.kind === 'file') continue;
    const pid = parentId.get(node.id) ?? null;
    const list = childIds.get(pid) ?? [];
    list.push(node.id);
    childIds.set(pid, list);
  }

  const byId = new Map(nodes.map((n) => [n.id, n]));

  function buildFromParent(pid: string | null): SymbolTreeNode[] {
    const ids = childIds.get(pid) ?? [];
    const children = ids
      .map((id) => byId.get(id))
      .filter((n): n is NodeRow => !!n)
      .sort(compareNodes);

    return children.map((node) => ({
      node,
      children: buildFromParent(node.id),
    }));
  }

  const roots = fileNode ? buildFromParent(fileNode.id) : buildFromParent(null);
  if (roots.length === 0) {
    const fallback = buildFromParent(null);
    if (fallback.length > 0) return fallback;
  }
  return roots;
}

export function sectionLabelForKind(kind: string): string {
  return KIND_SECTION_LABEL[kind] ?? kind;
}

export function shouldShowSectionHeader(
  nodes: SymbolTreeNode[],
  index: number,
): boolean {
  if (index === 0) return true;
  return nodes[index].node.kind !== nodes[index - 1].node.kind;
}
