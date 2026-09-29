import type {
  DisplayHit,
  MemoryMatch,
  PolicyRule,
  PolicySkill,
  SearchHit,
} from "./types";

export function matchesQuery(query: string, fields: Array<string | undefined>): boolean {
  const needle = query.trim().toLowerCase();
  if (!needle) return false;
  return fields.some((field) => (field ?? "").toLowerCase().includes(needle));
}

export function codeHits(rows: SearchHit[]): DisplayHit[] {
  return rows.map((row) => ({
    kind: "code",
    title: row.qualified_name || row.name,
    subtitle: `${row.file_path}:${row.start_line}`,
    body: [row.kind, row.snippet].filter(Boolean).join("\n"),
  }));
}

export function ruleHits(rules: PolicyRule[], query: string): DisplayHit[] {
  return rules
    .filter((rule) => matchesQuery(query, [rule.id, rule.level, rule.body, ...(rule.tags ?? [])]))
    .map((rule) => ({
      kind: "rule",
      title: rule.id,
      subtitle: rule.level,
      body: rule.body,
    }));
}

export function skillHits(skills: PolicySkill[], query: string): DisplayHit[] {
  return skills
    .filter((skill) =>
      matchesQuery(query, [skill.name, skill.description, skill.body, ...(skill.tags ?? [])]),
    )
    .map((skill) => ({
      kind: "skill",
      title: skill.name,
      subtitle: skill.description,
      body: skill.body,
    }));
}

export function memoryHits(matches: MemoryMatch[]): DisplayHit[] {
  return matches.map((match) => ({
    kind: "memory",
    title: match.title,
    subtitle: match.kind,
    body: match.body,
  }));
}
