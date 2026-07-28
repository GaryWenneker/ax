export type PolicyScope =
  | 'company'
  | 'workspace'
  | 'project'
  | 'private_user'
  | 'private_project';

export const POLICY_SCOPES: { value: PolicyScope; label: string }[] = [
  { value: 'company', label: 'Company' },
  { value: 'workspace', label: 'Workspace' },
  { value: 'project', label: 'Project' },
  { value: 'private_user', label: 'Private (user)' },
  { value: 'private_project', label: 'Private (project)' },
];

export function scopeLabel(scope?: string): string {
  const found = POLICY_SCOPES.find((s) => s.value === scope);
  return found?.label ?? (scope || 'Project');
}

export interface RuleFrontmatter {
  id: string;
  level: string;
  alwaysApply: boolean;
  globs: string[];
  triggers: string[];
  tags: string[];
  priority: number;
  enabled?: boolean;
  status?: string;
  share?: boolean;
  scope?: string;
}

export interface SkillFrontmatter {
  name: string;
  description: string;
  triggers: string[];
  tags: string[];
  priority: number;
  contextTask?: string;
  enabled?: boolean;
  status?: string;
  share?: boolean;
  scope?: string;
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
  enabled?: boolean;
  status?: string;
  scope?: string;
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
  enabled?: boolean;
  status?: string;
  scope?: string;
}

export interface PendingPolicyItem {
  kind: string;
  id: string;
  path: string;
  status: string;
  preview: string;
  levelOrDescription: string;
}

export interface PolicySyncSettings {
  policySync: boolean;
  requireReview: boolean;
  storage: string;
}

export interface PolicyPackStatus {
  packPath: string;
  hasManifest: boolean;
  rulesInPack: number;
  skillsInPack: number;
  exportedAt?: number | null;
  tag?: string | null;
  localSharedRules: number;
  localSharedSkills: number;
  requireReview: boolean;
  policySync: boolean;
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
