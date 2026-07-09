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

export interface CaptureInterviewQuestion {
  field: string;
  question: string;
  current: string;
  options: string[];
  required: boolean;
}

export interface CaptureProposal {
  detected: boolean;
  confidence: string;
  suggestedId: string;
  frontmatter: RuleFrontmatter;
  body: string;
  previewPath: string;
  preview: string;
  questions: CaptureInterviewQuestion[];
  interviewInstruction: string;
}

export interface CaptureProposeResult {
  ok: boolean;
  action: 'propose';
  detected: boolean;
  proposal?: CaptureProposal;
  preview?: string;
  questions?: CaptureInterviewQuestion[];
  instruction?: string;
}

export interface CaptureSaveResult {
  ok: boolean;
  action: 'save';
  id: string;
  storage: string;
  path: string;
}
