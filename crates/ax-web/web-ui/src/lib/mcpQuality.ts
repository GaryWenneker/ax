/** MCP quality snapshot types + API helpers for Command Center. */

export interface EnrichmentMetrics {
  injectCharsP50: number;
  injectCharsP95: number;
  enrichDoneRate: number;
  emptyEnrichCount: number;
  matchedRulesRate: number;
  preflightCount: number;
  inboundCount: number;
}

export interface ToolMix {
  preflight: number;
  explore: number;
  guard: number;
  graph: number;
  otherAx: number;
  read: number;
  grep: number;
}

export interface QualityFinding {
  id: string;
  check: string;
  severity: string;
  title: string;
  detail: string;
  wasteHint: string;
  tokensEst: number;
  tool?: string | null;
  tsMs?: number | null;
  logLineHint?: string | null;
}

export interface QualitySnapshot {
  projectRoot: string;
  projectLabel: string;
  logPath: string;
  mode: string;
  windowMinutes: number;
  updatedAtMs: number;
  score: number;
  grade: string;
  correlationPct: number;
  matchedCalls: number;
  unmatchedAxCalls: number;
  verboseClusters: number;
  enrichment: EnrichmentMetrics;
  toolMix: ToolMix;
  findings: QualityFinding[];
  tokensAtRisk: number;
  criticalCount: number;
  verboseEnabled: boolean;
  verbosePresent: boolean;
  sessionId?: string | null;
  sessionPath?: string | null;
}

export const MCP_QUALITY_URL = '/api/usage/mcp-quality';
export const MCP_QUALITY_EVENTS_URL = '/api/usage/mcp-quality/events';
export const MCP_AUDIT_URL = '/api/usage/mcp-audit';

export const MCP_QUALITY_OPEN = 'ax-mcp-quality-open';
export const MCP_QUALITY_FINDING = 'ax-mcp-quality-finding';

export function emptyQualitySnapshot(): QualitySnapshot {
  return {
    projectRoot: '',
    projectLabel: '—',
    logPath: '',
    mode: 'verbose_only',
    windowMinutes: 30,
    updatedAtMs: 0,
    score: 0,
    grade: '—',
    correlationPct: 0,
    matchedCalls: 0,
    unmatchedAxCalls: 0,
    verboseClusters: 0,
    enrichment: {
      injectCharsP50: 0,
      injectCharsP95: 0,
      enrichDoneRate: 0,
      emptyEnrichCount: 0,
      matchedRulesRate: 0,
      preflightCount: 0,
      inboundCount: 0,
    },
    toolMix: {
      preflight: 0,
      explore: 0,
      guard: 0,
      graph: 0,
      otherAx: 0,
      read: 0,
      grep: 0,
    },
    findings: [],
    tokensAtRisk: 0,
    criticalCount: 0,
    verboseEnabled: false,
    verbosePresent: false,
    sessionId: null,
    sessionPath: null,
  };
}

export async function fetchMcpQuality(): Promise<QualitySnapshot> {
  const res = await fetch(MCP_QUALITY_URL);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json() as Promise<QualitySnapshot>;
}

export async function runMcpAudit(opts?: {
  session?: string;
  windowMinutes?: number;
  markdown?: boolean;
}): Promise<QualitySnapshot & { markdown?: string }> {
  const res = await fetch(MCP_AUDIT_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      session: opts?.session,
      window_minutes: opts?.windowMinutes,
      markdown: opts?.markdown ?? false,
    }),
  });
  const data = (await res.json()) as QualitySnapshot & {
    markdown?: string;
    snapshot?: QualitySnapshot;
    error?: string;
  };
  if (!res.ok) throw new Error(data.error ?? `HTTP ${res.status}`);
  if (data.snapshot) {
    return { ...data.snapshot, markdown: data.markdown };
  }
  return data;
}

export function openMcpQualitySlideout(detail?: { findingId?: string }) {
  window.dispatchEvent(new CustomEvent(MCP_QUALITY_OPEN, { detail: detail ?? {} }));
}

export function gradeTone(score: number): 'ok' | 'warn' | 'bad' | 'muted' {
  if (score >= 80) return 'ok';
  if (score >= 60) return 'warn';
  if (score > 0) return 'bad';
  return 'muted';
}

function suggestedWork(check: string): string[] {
  switch (check) {
    case 'ExploreBeforeGrep':
      return [
        'Strengthen agent-workflow / explore-before-grep so agents call `ax_explore` (or graph tools) before Read/Grep for structural questions.',
        'Audit AGENTS.md and startup skill inject so the rule is hard to miss.',
        'Add or tighten a CRITICAL policy rule + `ax_guard` coverage if agents keep grepping the graph.',
      ];
    case 'EnrichPresent':
    case 'RulesInjected':
      return [
        'Investigate why `ax_preflight` sometimes returns empty inject (`inject_chars=0`) — policy store, projectPath, or verbose enrich logging.',
        'Ensure Command Center / MCP daemon use the correct workspace root so matched rules ship in inject.',
        'Add a regression check: preflight with this project must include `<ax_policy>` and non-empty inject.',
      ];
    case 'PreflightOnce':
      return [
        'Make once-per-turn preflight unambiguous in policy/skills (dedup wording in agent-workflow).',
        'If preflight is missing entirely, surface a stronger Logging/Quality CTA and MCP reconnect hint.',
      ];
    case 'VerboseGap':
    case 'UncorrelatedTool':
      return [
        'Improve transcript↔verbose correlation (Cursor `CallDynamicTool` + timestamps / optional session_id on verbose lines).',
        'Ensure Verbose MCP logging is on for the active project and MCP was restarted after enabling it.',
        'For `ax_guard` path-required errors: pass `path` (or `paths[]`) and `operation`/`action` — recovered retries no longer penalize the score.',
      ];
    case 'GuardBeforeWrite':
      return [
        'Reinforce `ax_guard` before Write/Delete when CRITICAL rules exist; verify guard appears in verbose clusters during edit sessions.',
      ];
    default:
      return [
        'Diagnose this finding with `ax mcp audit --json` and Logging Call Inspector, then ship a focused fix + docs.',
      ];
  }
}

/** Agent-ready Markdown fixpack from a quality snapshot (copy into Cursor chat). */
export function formatQualityFixpack(snap: QualitySnapshot): string {
  const lines: string[] = [];
  lines.push('# ax MCP quality fixpack');
  lines.push('');
  lines.push(
    'Paste this into an agent chat in the **ax** repo. Goal: raise the MCP quality score and cut token waste by fixing the findings below. Use ax MCP (`ax_preflight`, `ax_explore`, `ax_guard`) for the work.',
  );
  lines.push('');
  lines.push('## Snapshot');
  lines.push('');
  lines.push(`- **Project:** ${snap.projectLabel} (\`${snap.projectRoot || '—'}\`)`);
  lines.push(`- **Score:** ${snap.score} (${snap.grade}) · mode \`${snap.mode}\` · window ${snap.windowMinutes}m`);
  lines.push(
    `- **Correlation:** ${Math.round(snap.correlationPct)}% (${snap.matchedCalls} matched / ${snap.unmatchedAxCalls} unmatched)`,
  );
  lines.push(`- **Tokens at risk:** ~${snap.tokensAtRisk.toLocaleString()}`);
  lines.push(
    `- **Tool mix:** preflight ${snap.toolMix.preflight} · explore ${snap.toolMix.explore} · guard ${snap.toolMix.guard} · graph ${snap.toolMix.graph} · Read ${snap.toolMix.read} · Grep ${snap.toolMix.grep}`,
  );
  lines.push(
    `- **Enrichment:** inject p50/p95 ${snap.enrichment.injectCharsP50}/${snap.enrichment.injectCharsP95} · enrich-done ${Math.round(snap.enrichment.enrichDoneRate * 100)}% · empty ${snap.enrichment.emptyEnrichCount} · rules ${Math.round(snap.enrichment.matchedRulesRate * 100)}%`,
  );
  lines.push(`- **Verbose:** enabled=${snap.verboseEnabled} present=${snap.verbosePresent} · \`${snap.logPath || '—'}\``);
  if (snap.sessionId) lines.push(`- **Session:** \`${snap.sessionId}\``);
  lines.push('');
  lines.push('## Findings (priority order)');
  lines.push('');
  if (snap.findings.length === 0) {
    lines.push('_No open findings — keep quality monitoring on and avoid regressing explore-before-grep / preflight._');
    lines.push('');
  } else {
    snap.findings.forEach((f, i) => {
      lines.push(`### ${i + 1}. [${f.severity}] ${f.check} — ${f.title}`);
      lines.push('');
      lines.push(f.detail);
      lines.push('');
      lines.push(`*Waste:* ${f.wasteHint} (~${f.tokensEst.toLocaleString()} tokens)`);
      lines.push('');
      lines.push('**Suggested work:**');
      for (const s of suggestedWork(f.check)) {
        lines.push(`- ${s}`);
      }
      lines.push('');
    });
  }
  lines.push('## Acceptance');
  lines.push('');
  lines.push('1. Re-run `ax mcp audit` (or Refresh in the Quality slide-out) — score should rise; critical/high findings for this window should drop.');
  lines.push('2. Logging shows healthy enrich/preflight clusters; agents use `ax_explore` before broad Read/Grep for structural work.');
  lines.push('3. Update docs/README in the same change if you touch CLI/MCP/Command Center surfaces.');
  lines.push('');
  lines.push('## Verify');
  lines.push('');
  lines.push('```bash');
  lines.push('ax mcp audit');
  lines.push('ax mcp audit --json');
  lines.push('```');
  lines.push('');
  return lines.join('\n');
}

