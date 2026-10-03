/** Turns internal agent telemetry into user-facing activity copy. */

export interface ContextReport {
  session?: string;
  working?: string;
  stale: boolean;
  cache: number;
  tokensReused?: number;
  cacheRatio?: number;
}

export type ContextFace = 'ready' | 'cached' | 'refreshing' | 'rebuilt';

export interface ActivitySnapshot {
  phase: 'thinking' | 'preparing' | 'completed';
  startedAt: number;
  endedAt?: number;
  tools: number;
  searches: number;
  context?: ContextReport;
  /** True after a refreshing report was followed by a fresh one. */
  rebuilt: boolean;
}

const REPORT_RE = /<ax_context_report\b([^>]*)\/?>/gi;

export function emptyActivity(now = Date.now()): ActivitySnapshot {
  return {
    phase: 'thinking',
    startedAt: now,
    tools: 0,
    searches: 0,
    rebuilt: false,
  };
}

export function isSearchTool(name: string): boolean {
  const n = name.toLowerCase();
  return (
    n.includes('search') ||
    n.includes('explore') ||
    n === 'rg' ||
    n === 'grep' ||
    n.endsWith('/rg')
  );
}

export function stripContextReports(text: string): { text: string; reports: ContextReport[] } {
  const reports: ContextReport[] = [];
  const cleaned = text.replace(REPORT_RE, (_full, attrs: string) => {
    reports.push(parseAttrs(attrs));
    return '';
  });
  return { text: cleaned.replace(/^\s+/, ''), reports };
}

export function applyReport(snapshot: ActivitySnapshot, report: ContextReport): ActivitySnapshot {
  const wasRefreshing = snapshot.context?.stale === true;
  const rebuilt = snapshot.rebuilt || (wasRefreshing && !report.stale);
  return {
    ...snapshot,
    context: report,
    rebuilt,
    phase: snapshot.phase === 'completed' ? 'completed' : report.stale ? 'preparing' : 'thinking',
  };
}

export function noteTool(snapshot: ActivitySnapshot, name: string): ActivitySnapshot {
  return {
    ...snapshot,
    tools: snapshot.tools + 1,
    searches: snapshot.searches + (isSearchTool(name) ? 1 : 0),
    phase: snapshot.phase === 'completed' ? 'completed' : snapshot.phase,
  };
}

export function completeActivity(snapshot: ActivitySnapshot, now = Date.now()): ActivitySnapshot {
  return { ...snapshot, phase: 'completed', endedAt: snapshot.endedAt ?? now };
}

export function contextFace(snapshot: ActivitySnapshot): ContextFace | null {
  const report = snapshot.context;
  if (!report) return null;
  if (report.stale) return 'refreshing';
  if (snapshot.rebuilt) return 'rebuilt';
  if (report.cache > 0 || report.tokensReused != null || report.cacheRatio != null) return 'cached';
  return 'ready';
}

export function contextLabel(snapshot: ActivitySnapshot): string | null {
  const face = contextFace(snapshot);
  if (face === 'refreshing') return 'Context · Refreshing…';
  if (face === 'rebuilt') return 'Context · Rebuilt';
  if (face === 'cached') return cachedLabel(snapshot.context);
  if (face === 'ready') return 'Context · Ready';
  return null;
}

export function headline(snapshot: ActivitySnapshot, now = Date.now()): string {
  const elapsed = formatDuration(elapsedMs(snapshot, now));
  if (snapshot.phase === 'completed') return `Completed · ${elapsed}`;
  if (snapshot.phase === 'preparing') return 'Preparing context';
  return `Thinking · ${elapsed}`;
}

/** Text that is allowed in the normal activity surface. */
export function publicActivityText(snapshot: ActivitySnapshot, now = Date.now()): string {
  const parts = [headline(snapshot, now)];
  const context = contextLabel(snapshot);
  if (context) parts.push(context);
  if (snapshot.tools > 0) parts.push(String(snapshot.tools));
  if (snapshot.searches > 0) parts.push(String(snapshot.searches));
  if (snapshot.phase === 'completed') {
    const bits = ['Completed', formatDuration(elapsedMs(snapshot, now))];
    if (contextFace(snapshot) === 'cached') bits.push('Context cached');
    if (snapshot.tools > 0) bits.push(countPhrase(snapshot.tools, 'tool', 'tools'));
    if (snapshot.searches > 0) bits.push(countPhrase(snapshot.searches, 'search', 'searches'));
    parts.push(bits.join(' · '));
  }
  return parts.join('\n');
}

export function summaryLine(snapshot: ActivitySnapshot): string {
  const face = contextFace(snapshot);
  if (face === 'refreshing') return 'Refreshing context…';
  if (face === 'rebuilt') return 'Context rebuilt for this turn';
  if (face === 'cached') return 'Context is ready · cached context available';
  if (face === 'ready') return 'Context is ready';
  if (snapshot.phase === 'completed') return 'Turn finished';
  return '';
}

export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  if (minutes === 0) return `${seconds}s`;
  return `${minutes}m ${seconds}s`;
}

export function formatTokens(tokens: number): string {
  if (tokens >= 1000) {
    const scaled = tokens / 1000;
    const text = scaled >= 100 ? scaled.toFixed(0) : scaled.toFixed(1);
    return `${text}k tokens reused`;
  }
  return `${tokens} tokens reused`;
}

function cachedLabel(report: ContextReport | undefined): string {
  if (!report) return 'Context · Cached';
  if (report.cacheRatio != null && Number.isFinite(report.cacheRatio)) {
    const pct = Math.round(report.cacheRatio * (report.cacheRatio <= 1 ? 100 : 1));
    return `Context · Cached · ${pct}%`;
  }
  if (report.tokensReused != null && Number.isFinite(report.tokensReused)) {
    return `Context · Cached · ${formatTokens(report.tokensReused)}`;
  }
  return 'Context · Cached';
}

function countPhrase(count: number, singular: string, plural: string): string {
  return `${count} ${count === 1 ? singular : plural}`;
}

function elapsedMs(snapshot: ActivitySnapshot, now: number): number {
  return (snapshot.endedAt ?? now) - snapshot.startedAt;
}

function parseAttrs(raw: string): ContextReport {
  const attrs = new Map<string, string>();
  const re = /([A-Za-z_:][\w:.-]*)\s*=\s*"([^"]*)"/g;
  let match: RegExpExecArray | null;
  while ((match = re.exec(raw)) !== null) {
    attrs.set(match[1], match[2]);
  }
  const cache = Number(attrs.get('cache') ?? '0');
  const tokens = attrs.has('tokensReused') ? Number(attrs.get('tokensReused')) : undefined;
  const ratio = attrs.has('cacheRatio') ? Number(attrs.get('cacheRatio')) : undefined;
  return {
    session: attrs.get('session'),
    working: attrs.get('working'),
    stale: attrs.get('stale') === 'true',
    cache: Number.isFinite(cache) ? cache : 0,
    tokensReused: tokens != null && Number.isFinite(tokens) ? tokens : undefined,
    cacheRatio: ratio != null && Number.isFinite(ratio) ? ratio : undefined,
  };
}
