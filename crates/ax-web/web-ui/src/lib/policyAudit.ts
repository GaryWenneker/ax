/** Policy audits: one request, one summary and one agent brief for skills, rules and catalog rows. */

export type AuditFinding = { severity: string; check?: string; evidence?: string };

export type AuditReport = {
  name?: string;
  candidate?: boolean;
  edit_file?: string;
  mechanical?: AuditFinding[];
  verdict?: { verdict?: string } | null;
};

export type SecurityAudit = { name: string; status: string; url: string };

export type AuditOutcome = { summary: string; brief: string };

export type AuditSummary = { warns: number; fails: number; detail: string; securityLine: string; verdict: string };

type Fetcher = (url: string, init?: RequestInit) => Promise<Response>;

const defaultFetch: Fetcher = (url, init) => fetch(url, init);

/** The server's `error` or `message`, the plain text body, or `fallback` when the body is empty. */
export function messageFrom(text: string, fallback: string): string {
  const trimmed = text.trim();
  if (!trimmed) return fallback;
  try {
    const body = JSON.parse(trimmed) as { error?: unknown; message?: unknown };
    if (typeof body.error === 'string' && body.error.trim()) return body.error.trim();
    if (typeof body.message === 'string' && body.message.trim()) return body.message.trim();
  } catch {
    /* The server sometimes returns plain text. */
  }
  return trimmed;
}

/** POST the audit for one skill or rule and return its report. */
export async function runAudit(kind: 'skills' | 'rules', name: string, fetcher: Fetcher = defaultFetch): Promise<AuditReport> {
  const res = await fetcher(`/api/policy/${kind}/${encodeURIComponent(name)}/audit`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: '{}',
  });
  const text = await res.text();
  if (!res.ok) throw new Error(messageFrom(text, 'Audit failed'));
  return JSON.parse(text) as AuditReport;
}

/** Warn and fail counts (quality findings plus flagged security audits) and the first four findings. */
export function summarizeAudit(report: AuditReport, security: SecurityAudit[] = []): AuditSummary {
  const mechanical = report.mechanical ?? [];
  const flagged = security.filter((audit) => audit.status !== 'pass');
  const count = (severity: string) =>
    mechanical.filter((f) => f.severity === severity).length + flagged.filter((a) => a.status === severity).length;
  const lines = mechanical
    .slice(0, 4)
    .map((f) => `${f.severity} ${f.check ?? ''}: ${f.evidence ?? ''}`.trim())
    .filter(Boolean);
  return {
    warns: count('warn'),
    fails: count('fail'),
    detail: lines.length ? ` ${lines.join(' · ')}` : '',
    securityLine: flagged.length ? ` ${flagged.map((a) => `${a.name} ${a.status.toUpperCase()}`).join(', ')}.` : '',
    verdict: report.verdict?.verdict ? ` Verdict ${report.verdict.verdict}.` : '',
  };
}

export async function auditOneSkill(name: string, fetcher: Fetcher = defaultFetch): Promise<AuditOutcome> {
  const report = await runAudit('skills', name, fetcher);
  const s = summarizeAudit(report);
  return {
    summary: `Audited ${name}: ${s.warns} warn, ${s.fails} fail.${s.verdict}${s.detail} Skill stays usable.`,
    brief: agentBrief(report, []),
  };
}

export async function auditOneRule(id: string, fetcher: Fetcher = defaultFetch): Promise<AuditOutcome> {
  const report = await runAudit('rules', id, fetcher);
  const s = summarizeAudit(report);
  return {
    summary: `Audited ${id}: ${s.warns} warn, ${s.fails} fail.${s.detail} Rule stays enabled.`,
    brief: agentBrief(report, []),
  };
}

const EDIT_INSTRUCTION: Record<string, string> = {
  trigger: 'Rewrite the description so it says when to use this.',
  applies: 'Turn on alwaysApply, or add a trigger or glob.',
  id: 'Set the frontmatter id to the file name.',
  body: 'Add the steps the agent should follow after the frontmatter.',
  'missing-file': 'Add that file or remove the reference.',
  path: 'Remove the parent-directory path.',
  symlink: 'Replace the symlink with a real file.',
  name: 'Set the frontmatter name to the directory name.',
  frontmatter: 'Fix the frontmatter so name and description are valid.',
  truncated: 'Move extra detail into references/.',
};

/** Escape text so it cannot open or close a tag inside the brief. */
function text(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function attr(value: string): string {
  return text(value).replace(/"/g, '&quot;');
}

/**
 * An `<ax_audit>` block to paste into an agent. Finding evidence and security
 * audit fields come from third-party skills, so every inserted value is escaped.
 */
export function agentBrief(report: AuditReport, security: SecurityAudit[]): string {
  const file = report.edit_file || '';
  const kind = file.endsWith('.mdc') ? 'rule' : 'skill';
  const mechanical = (report.mechanical ?? []).filter(
    (finding) => finding.severity !== 'info' || finding.check === 'truncated',
  );
  const flagged = security.filter((audit) => audit.status !== 'pass');
  const lines: string[] = [];
  if (report.candidate) lines.push('Candidate only. Nothing was installed.');
  if (mechanical.length === 0 && flagged.length === 0) {
    lines.push('No edits.');
  } else {
    lines.push('Edit this file. Leave it enabled.');
    for (const finding of mechanical) {
      const instruction = EDIT_INSTRUCTION[finding.check ?? ''] ?? 'Apply this finding in the file.';
      lines.push(text(`- ${finding.severity} ${finding.check ?? ''}: ${finding.evidence ?? ''} ${instruction}`.trim()));
    }
    for (const audit of flagged) {
      lines.push(text(`- ${audit.status} security: ${audit.name} ${audit.status.toUpperCase()}. ${audit.url}`));
    }
  }
  return `<ax_audit kind="${kind}" name="${attr(report.name ?? '')}" file="${attr(file)}">\n${lines.join('\n')}\n</ax_audit>\n`;
}
