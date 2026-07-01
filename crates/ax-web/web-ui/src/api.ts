import type { Stats, NodeRow, NodeDetail, FileRow, SearchResult } from './types';

const BASE = '/api';

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) {
    const body = await res.json().catch(() => ({})) as { error?: string };
    throw new Error(body.error ?? `HTTP ${res.status}`);
  }
  return res.json() as Promise<T>;
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
  q?: string;
  limit?: number;
  offset?: number;
}): Promise<NodePage> {
  const sp = new URLSearchParams();
  if (params.kind) sp.set('kind', params.kind);
  if (params.lang) sp.set('lang', params.lang);
  if (params.q) sp.set('q', params.q);
  if (params.limit != null) sp.set('limit', String(params.limit));
  if (params.offset != null) sp.set('offset', String(params.offset));
  return get<NodePage>(`/nodes?${sp}`);
}

export function fetchNodeDetail(id: string): Promise<NodeDetail> {
  return get<NodeDetail>(`/node/${encodeURIComponent(id)}`);
}

export interface FilePage {
  files: FileRow[];
  total: number;
}

export function fetchFiles(params: {
  lang?: string;
  q?: string;
  limit?: number;
  offset?: number;
}): Promise<FilePage> {
  const sp = new URLSearchParams();
  if (params.lang) sp.set('lang', params.lang);
  if (params.q) sp.set('q', params.q);
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

export function fetchVersion(): Promise<{ version: string }> {
  return get<{ version: string }>('/version');
}
