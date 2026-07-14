import type { Stats, NodeRow, NodeDetail, FileRow, FileRoot, SearchResult, UnresolvedSummary, UnresolvedRow } from './types';

const BASE = '/api';

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) {
    const body = await res.json().catch(() => ({})) as { error?: string };
    throw new Error(body.error ?? `HTTP ${res.status}`);
  }
  return res.json() as Promise<T>;
}

async function post<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  const data = await res.json().catch(() => ({})) as T & { error?: string };
  if (!res.ok) {
    throw new Error(data.error ?? `HTTP ${res.status}`);
  }
  return data;
}

export function fetchStats(): Promise<Stats> {
  return get<Stats>('/stats');
}

export interface NodePage {
  nodes: NodeRow[];
  total: number;
}

export function fetchNodes(params: {
  kind?: string;
  lang?: string;
  file?: string;
  q?: string;
  limit?: number;
  offset?: number;
}): Promise<NodePage> {
  const sp = new URLSearchParams();
  if (params.kind) sp.set('kind', params.kind);
  if (params.lang) sp.set('lang', params.lang);
  if (params.file) sp.set('file', params.file);
  if (params.q) sp.set('q', params.q);
  if (params.limit != null) sp.set('limit', String(params.limit));
  if (params.offset != null) sp.set('offset', String(params.offset));
  return get<NodePage>(`/nodes?${sp}`);
}

export function fetchNodeDetail(id: string): Promise<NodeDetail> {
  return get<NodeDetail>(`/node/${encodeURIComponent(id)}`);
}

export interface SourceLine {
  no: number;
  text: string;
}

export interface SourceSlice {
  path: string;
  from: number;
  to: number;
  total_lines: number;
  lines: SourceLine[];
}

export interface MemoryRow {
  id: string;
  kind: string;
  title: string;
  body: string;
  tags: string[];
  files: string[];
  confidence: number;
  source: string;
  created_at: number;
  updated_at: number;
}

export interface MemoryMatch extends MemoryRow {
  score: number;
}

export interface GitCaptureResult {
  scanned: number;
  captured: number;
  skipped_existing: number;
  skipped_trivial: number;
}

export function fetchMemories(params: { limit?: number; offset?: number } = {}): Promise<{ memories: MemoryRow[]; total: number }> {
  const sp = new URLSearchParams();
  if (params.limit != null) sp.set('limit', String(params.limit));
  if (params.offset != null) sp.set('offset', String(params.offset));
  const qs = sp.toString();
  return get<{ memories: MemoryRow[]; total: number }>(`/memory${qs ? `?${qs}` : ''}`);
}

export function recallMemories(q: string, limit = 10): Promise<{ matches: MemoryMatch[] }> {
  const sp = new URLSearchParams({ q, limit: String(limit) });
  return get<{ matches: MemoryMatch[] }>(`/memory/recall?${sp}`);
}

export function createMemory(input: {
  title?: string;
  body: string;
  kind?: string;
  tags?: string[];
  files?: string[];
}): Promise<{ memory: MemoryRow; similar: MemoryMatch[] }> {
  return post<{ memory: MemoryRow; similar: MemoryMatch[] }>('/memory', input);
}

export async function updateMemory(id: string, input: {
  title: string;
  body: string;
  kind?: string;
  tags?: string[];
  files?: string[];
}): Promise<void> {
  const res = await fetch(`${BASE}/memory/${encodeURIComponent(id)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({})) as { error?: string };
    throw new Error(body.error ?? `HTTP ${res.status}`);
  }
}

export async function deleteMemory(id: string): Promise<void> {
  const res = await fetch(`${BASE}/memory/${encodeURIComponent(id)}`, { method: 'DELETE' });
  if (!res.ok) {
    const body = await res.json().catch(() => ({})) as { error?: string };
    throw new Error(body.error ?? `HTTP ${res.status}`);
  }
}

export function captureGitMemories(limit = 100): Promise<GitCaptureResult> {
  return post<GitCaptureResult>('/memory/capture-git', { limit });
}

export function fetchSource(params: {
  path: string;
  start?: number;
  end?: number;
  context?: number;
}): Promise<SourceSlice> {
  const sp = new URLSearchParams();
  sp.set('path', params.path);
  if (params.start != null) sp.set('start', String(params.start));
  if (params.end != null) sp.set('end', String(params.end));
  if (params.context != null) sp.set('context', String(params.context));
  return get<SourceSlice>(`/source?${sp}`);
}

export interface FilePage {
  files: FileRow[];
  total: number;
}

export interface FileRootsPage {
  roots: FileRoot[];
}

export function fetchFileRoots(): Promise<FileRootsPage> {
  return get<FileRootsPage>('/files/roots');
}

export function fetchFiles(params: {
  lang?: string;
  q?: string;
  prefix?: string;
  limit?: number;
  offset?: number;
}): Promise<FilePage> {
  const sp = new URLSearchParams();
  if (params.lang) sp.set('lang', params.lang);
  if (params.q) sp.set('q', params.q);
  if (params.prefix) sp.set('prefix', params.prefix);
  if (params.limit != null) sp.set('limit', String(params.limit));
  if (params.offset != null) sp.set('offset', String(params.offset));
  return get<FilePage>(`/files?${sp}`);
}

export interface SearchPage {
  results: SearchResult[];
}

export function fetchSearch(q: string, limit = 20): Promise<SearchPage> {
  const sp = new URLSearchParams({ q, limit: String(limit) });
  return get<SearchPage>(`/search?${sp}`);
}

export interface UnresolvedPage {
  refs: UnresolvedRow[];
  total: number;
}

export function fetchUnresolvedSummary(): Promise<UnresolvedSummary> {
  return get<UnresolvedSummary>('/unresolved/summary');
}

export function fetchUnresolved(params: {
  q?: string;
  kind?: string;
  limit?: number;
  offset?: number;
}): Promise<UnresolvedPage> {
  const sp = new URLSearchParams();
  if (params.q) sp.set('q', params.q);
  if (params.kind) sp.set('kind', params.kind);
  if (params.limit != null) sp.set('limit', String(params.limit));
  if (params.offset != null) sp.set('offset', String(params.offset));
  return get<UnresolvedPage>(`/unresolved?${sp}`);
}

export interface UnresolvedReconcileResult {
  ok: boolean;
  pruned?: {
    orphan_from_node: number;
    stale_file: number;
    malformed_generic: number;
    external_calls: number;
  };
  resolved?: number;
  remaining?: number;
  error?: string;
}

export function reconcileUnresolved(): Promise<UnresolvedReconcileResult> {
  return post<UnresolvedReconcileResult>('/unresolved/reconcile', {});
}

export function fetchVersion(): Promise<{ version: string }> {
  return get<{ version: string }>('/version');
}

export interface GraphNode {
  id: string;
  name: string;
  kind: string;
  file_path: string;
  community_id: number;
  community_label?: string;
  degree: number;
}

export interface GraphEdge {
  source: string;
  target: string;
  kind: string;
  confidence?: string;
}

export interface GraphPayload {
  nodes: GraphNode[];
  edges: GraphEdge[];
  total_nodes: number;
  truncated: boolean;
}

export function fetchGraph(params: { limit?: number; recompute?: boolean } = {}): Promise<GraphPayload> {
  const sp = new URLSearchParams();
  if (params.limit != null) sp.set('limit', String(params.limit));
  if (params.recompute) sp.set('recompute', 'true');
  const qs = sp.toString();
  return get<GraphPayload>(`/graph${qs ? `?${qs}` : ''}`);
}

export interface GraphStreamMeta {
  total_nodes: number;
  truncated: boolean;
  node_count: number;
  edge_count: number;
}

export interface GraphStreamHandlers {
  onMeta?: (meta: GraphStreamMeta) => void;
  onNodes?: (nodes: GraphNode[]) => void;
  onEdges?: (edges: GraphEdge[]) => void;
  onDone?: () => void;
}

/**
 * Stream the graph as Server-Sent Events so it can be rendered gradually.
 * Falls back to the one-shot `fetchGraph` if streaming is unavailable.
 * Returns an abort function that cancels the in-flight stream.
 */
export function streamGraph(
  params: { limit?: number; recompute?: boolean },
  handlers: GraphStreamHandlers,
): () => void {
  const ctrl = new AbortController();
  const sp = new URLSearchParams();
  if (params.limit != null) sp.set('limit', String(params.limit));
  if (params.recompute) sp.set('recompute', 'true');
  const qs = sp.toString();

  (async () => {
    try {
      const res = await fetch(`${BASE}/graph/stream${qs ? `?${qs}` : ''}`, {
        signal: ctrl.signal,
        headers: { Accept: 'text/event-stream' },
      });
      if (!res.ok || !res.body) {
        // Fallback: one-shot fetch, delivered through the same handlers.
        const p = await fetchGraph(params);
        handlers.onMeta?.({
          total_nodes: p.total_nodes,
          truncated: p.truncated,
          node_count: p.nodes.length,
          edge_count: p.edges.length,
        });
        handlers.onNodes?.(p.nodes);
        handlers.onEdges?.(p.edges);
        handlers.onDone?.();
        return;
      }
      const reader = res.body.getReader();
      const dec = new TextDecoder();
      let buf = '';
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buf += dec.decode(value, { stream: true });
        const parts = buf.split('\n\n');
        buf = parts.pop() ?? '';
        for (const part of parts) {
          for (const line of part.split('\n')) {
            if (!line.startsWith('data: ')) continue;
            let ev: { type: string; nodes?: GraphNode[]; edges?: GraphEdge[] } & GraphStreamMeta;
            try {
              ev = JSON.parse(line.slice(6));
            } catch {
              continue;
            }
            if (ev.type === 'meta') handlers.onMeta?.(ev);
            else if (ev.type === 'nodes' && ev.nodes) handlers.onNodes?.(ev.nodes);
            else if (ev.type === 'edges' && ev.edges) handlers.onEdges?.(ev.edges);
            else if (ev.type === 'done') handlers.onDone?.();
          }
        }
      }
    } catch (e) {
      if ((e as Error).name !== 'AbortError') throw e;
    }
  })();

  return () => ctrl.abort();
}

export interface GodNode {
  nodeId: string;
  name: string;
  qualifiedName: string;
  filePath: string;
  kind: string;
  inDegree: number;
  outDegree: number;
  degree: number;
}

export interface CommunitySummary {
  communityId: number;
  label: string;
  size: number;
  keyNodes: string[];
}

export interface SurprisingEdge {
  sourceId: string;
  targetId: string;
  sourceName: string;
  targetName: string;
  kind: string;
  confidence: string;
  sourceCommunity: number;
  targetCommunity: number;
  sourceModule: string;
  targetModule: string;
  score: number;
}

export interface GraphInsights {
  nodeCount: number;
  edgeCount: number;
  numCommunities: number;
  modularity: number;
  godNodes: GodNode[];
  communities: CommunitySummary[];
  surprisingConnections: SurprisingEdge[];
}

export function fetchInsights(resolution = 1.0): Promise<GraphInsights> {
  const sp = new URLSearchParams({ resolution: String(resolution) });
  return get<GraphInsights>(`/insights?${sp}`);
}

export interface ToolSavingsRow {
  tool: string;
  calls: number;
  graph_calls: number;
  failed_calls: number;
  tokens_saved_est: number;
  counterfactual_files: number;
  counterfactual_tokens_est: number;
  graph_response_tokens_est: number;
  avg_duration_ms: number;
}

export interface DailySavingsRow {
  date: string;
  tokens_saved_est: number;
  calls: number;
  graph_calls: number;
  failed_calls: number;
  counterfactual_files: number;
  counterfactual_tokens_est: number;
  graph_response_tokens_est: number;
  cost_saved_usd_est: number;
}

export interface ProjectSavingsRow {
  project: string;
  calls: number;
  graph_calls: number;
  tokens_saved_est: number;
  counterfactual_files: number;
}

export interface WeekdaySavingsRow {
  weekday: number;
  label: string;
  tokens_saved_est: number;
  calls: number;
  graph_calls: number;
}

export interface RecentCallRow {
  tool: string;
  project: string | null;
  tokens_saved_est: number;
  counterfactual_tokens_est: number;
  response_tokens_est: number;
  counterfactual_files: number;
  ok: boolean;
  savings_eligible: boolean;
  duration_ms: number | null;
  created_at: number;
}

export interface AgentSessionRow {
  agent: string;
  session_id: string;
  read_calls: number;
  grep_calls: number;
  ax_calls: number;
  session_input_tokens: number | null;
  session_output_tokens: number | null;
  model: string | null;
  session_cost_usd_est: number | null;
  mcp_calls_in_window: number;
  tokens_saved_in_window: number;
}

export interface SavingsAssumptions {
  exact_tokenizer: boolean;
  chars_per_token: number;
  tokens_per_line: number;
  avg_file_tokens: number;
  counterfactual_mode: string;
}

export interface PricingInfo {
  reference_model: string;
  input_per_mtok: number;
  output_per_mtok: number;
  source: 'default' | 'user';
  config_path: string;
}

export interface SavingsSummary {
  period: string;
  from: string;
  to: string;
  mcp_calls: number;
  graph_calls: number;
  failed_calls: number;
  tokens_saved_est: number;
  net_tokens_saved_est: number;
  counterfactual_files: number;
  counterfactual_tokens_est: number;
  graph_response_tokens_est: number;
  response_tokens_est: number;
  counterfactual_exact_files: number;
  cost_saved_usd_est: number;
  graph_response_cost_usd_est: number;
  counterfactual_cost_usd_est: number;
  policy_calls: number;
  success_rate_pct: number;
  avg_duration_ms: number;
  projects_active: number;
  clamp_tokens_absorbed: number;
  graph_calls_with_savings: number;
  pricing: PricingInfo;
  assumptions: SavingsAssumptions;
  by_tool: ToolSavingsRow[];
  by_project: ProjectSavingsRow[];
  by_weekday: WeekdaySavingsRow[];
  daily: DailySavingsRow[];
  recent_calls: RecentCallRow[];
  agent_sessions: AgentSessionRow[];
  db_path: string;
}

export function fetchSavings(params: {
  period: string;
  from?: string;
  to?: string;
}): Promise<SavingsSummary> {
  const sp = new URLSearchParams({ period: params.period });
  if (params.from) sp.set('from', params.from);
  if (params.to) sp.set('to', params.to);
  return get<SavingsSummary>(`/usage/savings?${sp}`);
}

export interface SavingsImportResult {
  claude_sessions: number;
  cursor_sessions: number;
  skipped: number;
}

export function importSavingsSessions(): Promise<SavingsImportResult> {
  return post<SavingsImportResult>('/usage/savings/import', {});
}
