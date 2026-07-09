import type { PolicyRuleRow, PolicySkillRow } from '../../policyTypes';

export type SortDir = 'asc' | 'desc';

const LEVEL_RANK: Record<string, number> = { CRITICAL: 0, WARNING: 1, INFO: 2 };

function cmpStr(a: string, b: string) {
  return a.localeCompare(b, undefined, { sensitivity: 'base' });
}

function cmpNum(a: number, b: number) {
  return a - b;
}

export function filterRules(
  rules: PolicyRuleRow[],
  { q, level, always }: { q: string; level: string; always: string },
) {
  const needle = q.trim().toLowerCase();
  return rules.filter((r) => {
    if (level && r.level !== level) return false;
    if (always === 'yes' && !r.alwaysApply) return false;
    if (always === 'no' && r.alwaysApply) return false;
    if (!needle) return true;
    const hay = [
      r.id,
      r.level,
      r.tags.join(' '),
      r.triggers.join(' '),
      r.globs.join(' '),
    ].join(' ').toLowerCase();
    return hay.includes(needle);
  });
}

export type RuleSortKey = 'id' | 'level' | 'priority' | 'globs' | 'triggers';

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
  { q }: { q: string },
) {
  const needle = q.trim().toLowerCase();
  return skills.filter((s) => {
    if (!needle) return true;
    const hay = [s.name, s.description, s.tags.join(' '), s.triggers.join(' ')].join(' ').toLowerCase();
    return hay.includes(needle);
  });
}

export type SkillSortKey = 'name' | 'priority' | 'triggers';

export function sortSkills(skills: PolicySkillRow[], key: SkillSortKey, dir: SortDir) {
  const sorted = [...skills].sort((a, b) => {
    let c = 0;
    switch (key) {
      case 'name':
        c = cmpStr(a.name, b.name);
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
