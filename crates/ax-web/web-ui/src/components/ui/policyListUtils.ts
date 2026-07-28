import type { PolicyRuleRow, PolicySkillRow } from '../../policyTypes';

export type SortDir = 'asc' | 'desc';

const LEVEL_RANK: Record<string, number> = { CRITICAL: 0, WARNING: 1, INFO: 2 };

function cmpStr(a: string, b: string) {
  return a.localeCompare(b, undefined, { sensitivity: 'base' });
}

function cmpNum(a: number, b: number) {
  return a - b;
}

function scopeOf(scope?: string) {
  return (scope || 'project').toLowerCase();
}

/** Unique tags across rows, sorted case-insensitively. */
export function collectTags(items: { tags: string[] }[]): string[] {
  const seen = new Map<string, string>();
  for (const item of items) {
    for (const tag of item.tags) {
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

export function filterRules(
  rules: PolicyRuleRow[],
  {
    q,
    level,
    always,
    scope,
    tags,
  }: { q: string; level: string; always: string; scope?: string; tags?: string[] },
) {
  const needle = q.trim().toLowerCase();
  const selectedTags = tags ?? [];
  return rules.filter((r) => {
    if (level && r.level !== level) return false;
    if (scope && scopeOf(r.scope) !== scope) return false;
    if (!hasAllTags(r.tags, selectedTags)) return false;
    if (always === 'yes' && !r.alwaysApply) return false;
    if (always === 'no' && r.alwaysApply) return false;
    if (!needle) return true;
    const hay = [
      r.id,
      r.level,
      scopeOf(r.scope),
      r.tags.join(' '),
      r.triggers.join(' '),
      r.globs.join(' '),
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
        c = cmpNum(a.globs.length, b.globs.length);
        break;
      case 'triggers':
        c = cmpNum(a.triggers.length, b.triggers.length);
        break;
    }
    return dir === 'asc' ? c : -c;
  });
  return sorted;
}

export function filterSkills(
  skills: PolicySkillRow[],
  { q, scope, tags }: { q: string; scope?: string; tags?: string[] },
) {
  const needle = q.trim().toLowerCase();
  const selectedTags = tags ?? [];
  return skills.filter((s) => {
    if (scope && scopeOf(s.scope) !== scope) return false;
    if (!hasAllTags(s.tags, selectedTags)) return false;
    if (!needle) return true;
    const hay = [
      s.name,
      s.description,
      scopeOf(s.scope),
      s.tags.join(' '),
      s.triggers.join(' '),
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
        c = cmpNum(a.triggers.length, b.triggers.length);
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
