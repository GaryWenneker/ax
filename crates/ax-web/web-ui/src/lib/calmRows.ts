export interface RuleRowFacts {
  alwaysApply?: boolean;
  globs?: string[];
  triggers?: string[];
  priority?: number;
  effectiveStorage?: string;
  storageIsOverride?: boolean;
}

export interface SkillRowFacts {
  description?: string;
  triggers?: string[];
  priority?: number;
  effectiveStorage?: string;
  storageIsOverride?: boolean;
}

function count(n: number, noun: string): string {
  if (n === 0) return '';
  return `${n} ${noun}${n === 1 ? '' : 's'}`;
}

function storageLabel(effective: string | undefined, projectStorage: string, override?: boolean): string {
  const label = (effective ?? projectStorage) === 'database' ? 'DB' : 'MD';
  return override ? `${label} override` : label;
}

function joinParts(parts: string[]): string {
  return parts.filter(Boolean).join(' · ');
}

export function ruleRowSubtitle(r: RuleRowFacts, projectStorage: string): string {
  return joinParts([
    r.alwaysApply ? 'always apply' : 'conditional',
    count((r.globs ?? []).length, 'glob'),
    count((r.triggers ?? []).length, 'trigger'),
    storageLabel(r.effectiveStorage, projectStorage, r.storageIsOverride),
  ]);
}

export function skillRowSubtitle(s: SkillRowFacts, projectStorage: string): string {
  const firstLine = (s.description ?? '').split('\n')[0].trim();
  return joinParts([
    firstLine,
    count((s.triggers ?? []).length, 'trigger'),
    storageLabel(s.effectiveStorage, projectStorage, s.storageIsOverride),
  ]);
}

export function priorityMeta(priority: number | undefined): string {
  return priority == null ? '' : `p${priority}`;
}

export type TraceDirection = 'in' | 'out' | 'internal';

export function traceDirection(kind: string): TraceDirection {
  if (kind === 'inbound') return 'in';
  if (kind === 'outbound' || kind === 'preview') return 'out';
  return 'internal';
}

const DIRECTION_LABELS: Record<TraceDirection, string> = {
  in: 'Prompt in',
  out: 'Returned to agent',
  internal: '',
};

export function traceDirectionLabel(dir: TraceDirection): string {
  return DIRECTION_LABELS[dir];
}

export function formatTraceTime(time: string): string {
  return time.replace(/\.\d+$/, '');
}

export function traceRowSubtitle(tool: string | undefined, kindLabel: string, meta: string): string {
  return joinParts([tool ?? '', kindLabel, meta]);
}
