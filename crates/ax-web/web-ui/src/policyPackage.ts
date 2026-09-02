export function allIdsSelected(ids: string[], selected: Set<string>): boolean {
  return ids.length > 0 && ids.every((id) => selected.has(id));
}

/** Select all when any are missing; otherwise clear (Select none). */
export function toggleSelectAll(ids: string[], selected: Set<string>): Set<string> {
  if (allIdsSelected(ids, selected)) return new Set();
  return new Set(ids);
}

export function compareLabel(compare: string): string {
  switch (compare) {
    case 'new':
      return 'New';
    case 'identical':
      return 'Identical';
    case 'changed':
      return 'Different';
    case 'invalid':
      return 'Invalid';
    default:
      return compare;
  }
}

export function policyItemDescription(input: {
  id: string;
  description?: string | null;
  body?: string | null;
}): string {
  const explicit = (input.description ?? '').trim();
  if (explicit) return clipDescription(explicit);
  const fromBody = firstProse(input.body ?? '');
  if (fromBody) return fromBody;
  return humanizePolicyId(input.id);
}

function clipDescription(text: string): string {
  const t = text.replace(/\s+/g, ' ').trim();
  if (t.length <= 220) return t;
  return `${t.slice(0, 217).trimEnd()}…`;
}

function firstProse(body: string): string | null {
  let para = '';
  for (const line of body.split('\n')) {
    let t = line.trim();
    if (!t) {
      if (para) break;
      continue;
    }
    if (t.startsWith('```')) continue;
    t = t.replace(/^#+/, '').replace(/^>/, '').trim();
    if (!t) continue;
    para = para ? `${para} ${t}` : t;
    if (para.length > 220) break;
  }
  return para ? clipDescription(para) : null;
}

function humanizePolicyId(id: string): string {
  return id.replace(/[-_]+/g, ' ').trim() || id;
}

export function compareBadgeClass(compare: string): string {
  const kind = ['new', 'identical', 'changed', 'invalid'].includes(compare) ? compare : 'invalid';
  return `policy-pack-badge policy-pack-badge--${kind}`;
}

export type UnifiedDiffLineKind = 'meta' | 'add' | 'del' | 'ctx';

export function unifiedDiffLines(unified: string): Array<{ kind: UnifiedDiffLineKind; text: string }> {
  if (!unified) return [];
  const raw = unified.replace(/\n$/, '').split('\n');
  return raw.map((line) => {
    if (line.startsWith('+++') || line.startsWith('---') || line.startsWith('@@')) {
      return { kind: 'meta' as const, text: line };
    }
    if (line.startsWith('+')) return { kind: 'add' as const, text: line };
    if (line.startsWith('-')) return { kind: 'del' as const, text: line };
    return { kind: 'ctx' as const, text: line };
  });
}
