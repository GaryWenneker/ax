import catalogJson from './skill-groups.json';
import type { PolicyRuleRow, PolicySkillRow } from './policyTypes';

export interface SkillGroupDef {
  id: string;
  label: string;
  order: number;
  aliases?: string[];
}

export const SKILL_GROUPS: SkillGroupDef[] = [...catalogJson].sort((a, b) => a.order - b.order);

export function skillGroupLabel(id: string): string {
  return SKILL_GROUPS.find((g) => g.id === id)?.label ?? id;
}

export function resolveSkillGroup(explicit: string | undefined | null, name: string, tags: string[]): string {
  const trimmed = (explicit ?? '').trim();
  if (trimmed) {
    return SKILL_GROUPS.some((g) => g.id === trimmed) ? trimmed : 'ungrouped';
  }
  const nameL = name.trim().toLowerCase();
  const tagL = tags.map((t) => t.trim().toLowerCase()).filter(Boolean);
  for (const g of SKILL_GROUPS) {
    if (g.id === 'ungrouped') continue;
    for (const alias of g.aliases ?? []) {
      const al = alias.toLowerCase();
      if (nameL === al || tagL.includes(al)) return g.id;
    }
  }
  return 'ungrouped';
}

export function skillResolvedGroup(skill: PolicySkillRow): string {
  if (skill.group && SKILL_GROUPS.some((g) => g.id === skill.group)) {
    return skill.group;
  }
  return resolveSkillGroup(skill.group, skill.name, skill.tags ?? []);
}

export function ruleResolvedGroup(rule: PolicyRuleRow): string {
  if (rule.group && SKILL_GROUPS.some((g) => g.id === rule.group)) {
    return rule.group;
  }
  return resolveSkillGroup(rule.group, rule.id, rule.tags ?? []);
}

/** Catalog groups that have at least one skill, in catalog order. Empty groups omitted. */
export function visibleSkillGroups(skills: PolicySkillRow[]): Array<SkillGroupDef & { skills: PolicySkillRow[] }> {
  const buckets = new Map<string, PolicySkillRow[]>();
  for (const skill of skills) {
    const id = skillResolvedGroup(skill);
    const list = buckets.get(id) ?? [];
    list.push(skill);
    buckets.set(id, list);
  }
  return SKILL_GROUPS.filter((g) => (buckets.get(g.id) ?? []).length > 0).map((g) => ({
    ...g,
    skills: buckets.get(g.id) ?? [],
  }));
}

/** Catalog groups that have at least one rule, in catalog order. Empty groups omitted. */
export function visibleRuleGroups(rules: PolicyRuleRow[]): Array<SkillGroupDef & { rules: PolicyRuleRow[] }> {
  const buckets = new Map<string, PolicyRuleRow[]>();
  for (const rule of rules) {
    const id = ruleResolvedGroup(rule);
    const list = buckets.get(id) ?? [];
    list.push(rule);
    buckets.set(id, list);
  }
  return SKILL_GROUPS.filter((g) => (buckets.get(g.id) ?? []).length > 0).map((g) => ({
    ...g,
    rules: buckets.get(g.id) ?? [],
  }));
}

export function toggleCollapsed(collapsed: Set<string>, id: string): Set<string> {
  const next = new Set(collapsed);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}
