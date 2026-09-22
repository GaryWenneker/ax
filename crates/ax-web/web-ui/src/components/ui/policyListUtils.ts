import type { PolicyRuleRow, PolicySkillRow } from '../../policyTypes';

export type SortDir = 'asc' | 'desc';

const LEVEL_RANK: Record<string, number> = { CRITICAL: 0, WARNING: 1, INFO: 2 };

function cmpStr(a: string | undefined | null, b: string | undefined | null) {
  return (a ?? '').localeCompare(b ?? '', undefined, { sensitivity: 'base' });
}

function cmpNum(a: number, b: number) {
  return a - b;
}

function scopeOf(scope?: string) {
  return (scope || 'project').trim().toLowerCase();
}

/** Normalized policy layer value for filters and comparisons. */
export function normalizePolicyScope(scope?: string) {
  return scopeOf(scope);
}

/** Layers that live under `.agents/` and are exportable in a git pack. */
const GIT_SHARED_SCOPES = new Set(['project', 'workspace']);

/**
 * True when a rule/skill is git-shared: enabled and project or workspace scope.
 * Matches packable + enabled (private, company, and disabled are not shared).
 */
export function isGitShared(scope?: string, enabled?: boolean): boolean {
  if (enabled === false) return false;
  return GIT_SHARED_SCOPES.has(normalizePolicyScope(scope));
}

/** Unique tags across rows, sorted case-insensitively. */
export function collectTags(items: { tags?: string[] }[]): string[] {
  const seen = new Map<string, string>();
  for (const item of items) {
    for (const tag of item.tags ?? []) {
      const key = tag.trim().toLowerCase();
      if (!key || seen.has(key)) continue;
      seen.set(key, tag.trim());
    }
  }
  return [...seen.values()].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }));
}

function hasTag(tags: string[], tag: string) {
  const needle = tag.trim().toLowerCase();
  return tags.some((t) => t.trim().toLowerCase() === needle);
}

/** True when every selected label is present on the item (AND). */
export function hasAllTags(itemTags: string[], selected: string[]) {
  if (selected.length === 0) return true;
  return selected.every((t) => hasTag(itemTags, t));
}

export function isGlobalPolicy(item: { origin?: string }): boolean {
  return item.origin === 'global';
}

/** Relocate stores Policy*Doc JSON (`frontmatter`). List rows must be flat. */
export function hydratePolicyListItem<T extends Record<string, unknown>>(raw: T): T {
  const fm =
    raw && typeof raw.frontmatter === 'object' && raw.frontmatter !== null
      ? (raw.frontmatter as Record<string, unknown>)
      : {};
  const tags = Array.isArray(raw.tags) ? raw.tags : Array.isArray(fm.tags) ? fm.tags : [];
  const triggers = Array.isArray(raw.triggers)
    ? raw.triggers
    : Array.isArray(fm.triggers)
      ? fm.triggers
      : [];
  const globs = Array.isArray(raw.globs) ? raw.globs : Array.isArray(fm.globs) ? fm.globs : [];
  return {
    ...fm,
    ...raw,
    name: typeof raw.name === 'string' && raw.name ? raw.name : typeof fm.name === 'string' ? fm.name : raw.name,
    id: typeof raw.id === 'string' && raw.id ? raw.id : typeof fm.id === 'string' ? fm.id : raw.id,
    description:
      typeof raw.description === 'string'
        ? raw.description
        : typeof fm.description === 'string'
          ? fm.description
          : raw.description,
    tags,
    triggers,
    globs,
  };
}

export function originQs(q?: { origin?: string; projectId?: number }): string {
  if (q?.origin !== 'global') return '';
  const p = new URLSearchParams();
  p.set('origin', 'global');
  if (q.projectId != null) p.set('projectId', String(q.projectId));
  return `?${p.toString()}`;
}

export type PolicyMenuId =
  | 'open'
  | 'edit'
  | 'enable'
  | 'disable'
  | 'move-global'
  | 'move-project'
  | 'delete';

export function policyOverviewMenuItems(origin: string | undefined, enabled: boolean | undefined): {
  id: PolicyMenuId;
  label: string;
  danger?: boolean;
}[] {
  if (origin === 'global') {
    return [
      { id: 'open', label: 'Open' },
      { id: 'edit', label: 'Edit' },
      { id: 'move-project', label: 'Move to this project (ax.db)' },
      { id: 'delete', label: 'Delete from global.db', danger: true },
    ];
  }
  const on = enabled !== false;
  return [
    { id: 'open', label: 'Open' },
    { id: 'edit', label: 'Edit' },
    { id: on ? 'disable' : 'enable', label: on ? 'Disable' : 'Enable' },
    { id: 'move-global', label: 'Move to global.db' },
    { id: 'delete', label: 'Delete', danger: true },
  ];
}

/** Solid fill for this project ax.db vs ~/.ax/global.db (WCAG dark ink). */
export const PROJECT_DB_COLOR = '#3ee4b2';
export const GLOBAL_DB_COLOR = '#e0b341';
export const POLICY_DB_INK = '#141414';

export function policyDbAccent(origin?: string): string {
  return origin === 'global' ? GLOBAL_DB_COLOR : PROJECT_DB_COLOR;
}

export function policyDbRowStyle(origin?: string): { boxShadow: string } {
  return { boxShadow: `inset 5px 0 0 ${policyDbAccent(origin)}` };
}

export function filterRules(
  rules: PolicyRuleRow[],
  {
    q,
    level,
    always,
    scope,
    tags,
    origin,
  }: { q: string; level: string; always: string; scope?: string; tags?: string[]; origin?: string },
) {
  const needle = q.trim().toLowerCase();
  const selectedTags = tags ?? [];
  return rules.filter((r) => {
    if (origin && (r.origin ?? 'project') !== origin) return false;
    if (level && r.level !== level) return false;
    if (scope && scopeOf(r.scope) !== scope) return false;
    if (!hasAllTags(r.tags ?? [], selectedTags)) return false;
    if (always === 'yes' && !r.alwaysApply) return false;
    if (always === 'no' && r.alwaysApply) return false;
    if (!needle) return true;
    const hay = [
      r.id,
      r.level,
      scopeOf(r.scope),
      (r.tags ?? []).join(' '),
      (r.triggers ?? []).join(' '),
      (r.globs ?? []).join(' '),
      r.projectName ?? '',
      r.origin ?? 'project',
    ].join(' ').toLowerCase();
    return hay.includes(needle);
  });
}

export type RuleSortKey = 'id' | 'level' | 'scope' | 'priority' | 'globs' | 'triggers';

export function sortRules(rules: PolicyRuleRow[], key: RuleSortKey, dir: SortDir) {
  const sorted = [...rules].sort((a, b) => {
    let c = 0;
    switch (key) {
      case 'id':
        c = cmpStr(a.id, b.id);
        break;
      case 'level':
        c = cmpNum(LEVEL_RANK[a.level] ?? 9, LEVEL_RANK[b.level] ?? 9);
        break;
      case 'scope':
        c = cmpStr(scopeOf(a.scope), scopeOf(b.scope));
        break;
      case 'priority':
        c = cmpNum(a.priority, b.priority);
        break;
      case 'globs':
        c = cmpNum((a.globs ?? []).length, (b.globs ?? []).length);
        break;
      case 'triggers':
        c = cmpNum((a.triggers ?? []).length, (b.triggers ?? []).length);
        break;
    }
    return dir === 'asc' ? c : -c;
  });
  return sorted;
}

export function filterSkills(
  skills: PolicySkillRow[],
  { q, scope, tags, origin }: { q: string; scope?: string; tags?: string[]; origin?: string },
) {
  const needle = q.trim().toLowerCase();
  const selectedTags = tags ?? [];
  return skills.filter((s) => {
    if (origin && (s.origin ?? 'project') !== origin) return false;
    if (scope && scopeOf(s.scope) !== scope) return false;
    if (!hasAllTags(s.tags ?? [], selectedTags)) return false;
    if (!needle) return true;
    const hay = [
      s.name,
      s.description,
      scopeOf(s.scope),
      (s.tags ?? []).join(' '),
      (s.triggers ?? []).join(' '),
      s.projectName ?? '',
      s.origin ?? 'project',
    ].join(' ').toLowerCase();
    return hay.includes(needle);
  });
}

export type SkillSortKey = 'name' | 'scope' | 'priority' | 'triggers';

export function sortSkills(skills: PolicySkillRow[], key: SkillSortKey, dir: SortDir) {
  const sorted = [...skills].sort((a, b) => {
    let c = 0;
    switch (key) {
      case 'name':
        c = cmpStr(a.name, b.name);
        break;
      case 'scope':
        c = cmpStr(scopeOf(a.scope), scopeOf(b.scope));
        break;
      case 'priority':
        c = cmpNum(a.priority, b.priority);
        break;
      case 'triggers':
        c = cmpNum((a.triggers ?? []).length, (b.triggers ?? []).length);
        break;
    }
    return dir === 'asc' ? c : -c;
  });
  return sorted;
}

export function toggleSort<T extends string>(
  current: T,
  dir: SortDir,
  key: T,
): { key: T; dir: SortDir } {
  if (current === key) return { key, dir: dir === 'asc' ? 'desc' : 'asc' };
  return { key, dir: 'asc' };
}
