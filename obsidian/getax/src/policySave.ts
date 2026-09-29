export interface RuleDraft {
  id: string;
  level: string;
  body: string;
  enabled: boolean;
  tags: string[];
  alwaysApply: boolean;
  globs: string[];
  triggers: string[];
  priority: number;
  status: string;
  share: boolean;
  scope: string;
}

export interface SkillDraft {
  name: string;
  description: string;
  body: string;
  enabled: boolean;
  tags: string[];
  alwaysApply: boolean;
  triggers: string[];
  priority: number;
  status: string;
  share: boolean;
  scope: string;
}

export function rulePayload(draft: RuleDraft): { frontmatter: Record<string, unknown>; body: string } {
  return {
    frontmatter: {
      id: draft.id.trim(),
      level: draft.level.trim() || "NORMAL",
      always_apply: draft.alwaysApply,
      globs: draft.globs,
      triggers: draft.triggers,
      tags: draft.tags,
      priority: draft.priority,
      enabled: draft.enabled,
      status: draft.status || "approved",
      share: draft.share,
      scope: draft.scope || "project",
    },
    body: draft.body,
  };
}

export function skillPayload(draft: SkillDraft): { frontmatter: Record<string, unknown>; body: string } {
  return {
    frontmatter: {
      name: draft.name.trim(),
      description: draft.description.trim(),
      always_apply: draft.alwaysApply,
      triggers: draft.triggers,
      tags: draft.tags,
      priority: draft.priority,
      enabled: draft.enabled,
      status: draft.status || "approved",
      share: draft.share,
      scope: draft.scope || "project",
    },
    body: draft.body,
  };
}

export function draftFromRule(frontmatter: Record<string, unknown>, body: string): RuleDraft {
  return {
    id: text(frontmatter.id),
    level: text(frontmatter.level) || "NORMAL",
    body,
    enabled: flag(frontmatter.enabled, true),
    tags: list(frontmatter.tags),
    alwaysApply: flag(frontmatter.alwaysApply ?? frontmatter.always_apply, false),
    globs: list(frontmatter.globs),
    triggers: list(frontmatter.triggers),
    priority: number(frontmatter.priority, 50),
    status: text(frontmatter.status) || "approved",
    share: flag(frontmatter.share, false),
    scope: text(frontmatter.scope) || "project",
  };
}

export function draftFromSkill(frontmatter: Record<string, unknown>, body: string): SkillDraft {
  return {
    name: text(frontmatter.name),
    description: text(frontmatter.description),
    body,
    enabled: flag(frontmatter.enabled, true),
    tags: list(frontmatter.tags),
    alwaysApply: flag(frontmatter.alwaysApply ?? frontmatter.always_apply, false),
    triggers: list(frontmatter.triggers),
    priority: number(frontmatter.priority, 50),
    status: text(frontmatter.status) || "approved",
    share: flag(frontmatter.share, false),
    scope: text(frontmatter.scope) || "project",
  };
}

function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function flag(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function number(value: unknown, fallback: number): number {
  return typeof value === "number" ? value : fallback;
}

function list(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}
