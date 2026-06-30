export interface RuleFrontmatter {
  id: string;
  level: string;
  alwaysApply: boolean;
  globs: string[];
  triggers: string[];
  tags: string[];
  priority: number;
}

export interface SkillFrontmatter {
  name: string;
  description: string;
  triggers: string[];
  tags: string[];
  priority: number;
  contextTask?: string;
}

export interface PolicyRuleDoc {
  frontmatter: RuleFrontmatter;
  body: string;
  raw: string;
  sourcePath: string;
}

export interface PolicySkillDoc {
  frontmatter: SkillFrontmatter;
  body: string;
  raw: string;
  sourcePath: string;
}

export interface PolicyRuleRow {
  id: string;
  level: string;
  alwaysApply: boolean;
  globs: string[];
  triggers: string[];
  tags: string[];
  priority: number;
  body: string;
  sourcePath: string;
}

export interface PolicySkillRow {
  name: string;
  description: string;
  triggers: string[];
  tags: string[];
  priority: number;
  contextTask?: string;
  body: string;
  sourcePath: string;
}

export interface MatchResult {
  rules: Array<{ id: string; level: string; score: number; reason: string; body: string }>;
  skills: Array<{ name: string; score: number; reason: string; description: string; body: string }>;
  inject: string;
}
