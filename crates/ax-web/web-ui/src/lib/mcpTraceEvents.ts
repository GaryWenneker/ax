import { entryHasQueryPayload, type TraceEntry, type TraceKind } from './mcpTrace';

export const MCP_TRACE_STATS = 'ax-mcp-trace-stats';
export const MCP_TRACE_FILTER = 'ax-mcp-trace-filter';
/** Commands from StatusBar → Logging page (e.g. jump to newest). */
export const MCP_TRACE_ACTION = 'ax-mcp-trace-action';

export type McpTraceActionDetail = {
  jumpToNew?: boolean;
};

export function publishMcpTraceAction(detail: McpTraceActionDetail) {
  window.dispatchEvent(new CustomEvent(MCP_TRACE_ACTION, { detail }));
}

export type McpTraceFilterDetail = {
  /** Replace kind multi-select (empty / omit = all kinds). */
  kinds?: TraceKind[];
  /** Toggle one kind in/out of the multi-select. */
  toggleKind?: TraceKind;
  /** Exact tool name, or '' to clear. */
  tool?: string;
  /** Calendar day `YYYY-MM-DD`, or '' to clear. */
  date?: string;
  /** Free-text query, or '' to clear. */
  q?: string;
  /**
   * When true, keep only lines whose JSON payload has a top-level `query`
   * property. When false / omit, do not filter on that flag.
   */
  hasQuery?: boolean;
  /** Reset all filters. */
  clear?: boolean;
};

export type McpTraceStats = {
  total: number;
  inbound: number;
  outbound: number;
  preview: number;
  error: number;
  internal: number;
  enrich: number;
  other: number;
  live: boolean;
  projectLabel: string;
  projectRoot: string;
  path: string;
};

const EMPTY_COUNTS: Record<TraceKind, number> = {
  inbound: 0,
  outbound: 0,
  preview: 0,
  error: 0,
  internal: 0,
  enrich: 0,
  plugin: 0,
  lsp: 0,
  ship: 0,
  share: 0,
  workspace: 0,
  embed: 0,
  action: 0,
  other: 0,
};

export const TRACE_KIND_ORDER: TraceKind[] = [
  'inbound',
  'outbound',
  'preview',
  'error',
  'internal',
  'enrich',
  'plugin',
  'lsp',
  'ship',
  'share',
  'workspace',
  'embed',
  'action',
  'other',
];

export function computeMcpTraceStats(
  entries: TraceEntry[],
  meta: {
    live: boolean;
    projectLabel: string;
    projectRoot: string;
    path: string;
  },
): McpTraceStats {
  const counts = { ...EMPTY_COUNTS };
  for (const e of entries) {
    counts[e.kind] = (counts[e.kind] ?? 0) + 1;
  }
  return {
    total: entries.length,
    inbound: counts.inbound,
    outbound: counts.outbound,
    preview: counts.preview,
    error: counts.error,
    internal: counts.internal,
    enrich: counts.enrich,
    other: counts.other,
    live: meta.live,
    projectLabel: meta.projectLabel,
    projectRoot: meta.projectRoot,
    path: meta.path,
  };
}

export function publishMcpTraceStats(stats: McpTraceStats) {
  window.dispatchEvent(new CustomEvent(MCP_TRACE_STATS, { detail: stats }));
}

export function publishMcpTraceFilter(detail: McpTraceFilterDetail) {
  window.dispatchEvent(new CustomEvent(MCP_TRACE_FILTER, { detail }));
}

export function emptyMcpTraceStats(): McpTraceStats {
  return {
    total: 0,
    inbound: 0,
    outbound: 0,
    preview: 0,
    error: 0,
    internal: 0,
    enrich: 0,
    other: 0,
    live: false,
    projectLabel: '—',
    projectRoot: '',
    path: '',
  };
}

/** Apply Logging table filters (kind multi-select + date + tool + text + query-payload). */
export function filterTraceEntries(
  entries: TraceEntry[],
  opts: {
    kinds: ReadonlySet<TraceKind>;
    tool: string;
    q: string;
    date?: string;
    /** Keep only entries with a JSON payload that owns a top-level `query` key. */
    hasQuery?: boolean;
  },
): TraceEntry[] {
  const tool = opts.tool.trim();
  const date = (opts.date ?? '').trim();
  const needle = opts.q.trim().toLowerCase();
  const kindsActive = opts.kinds.size > 0;
  const hasQuery = Boolean(opts.hasQuery);
  if (!kindsActive && !tool && !needle && !date && !hasQuery) return entries;
  return entries.filter((e) => {
    if (kindsActive && !opts.kinds.has(e.kind)) return false;
    if (date && e.day !== date) return false;
    if (tool && e.tool !== tool) return false;
    if (hasQuery && !entryHasQueryPayload(e)) return false;
    if (needle) {
      const hay = `${e.raw}\n${e.tool ?? ''}\n${e.message}`.toLowerCase();
      if (!hay.includes(needle)) return false;
    }
    return true;
  });
}
