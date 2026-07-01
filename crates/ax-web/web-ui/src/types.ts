export interface LangStat {
  language: string;
  count: number;
}

export interface Stats {
  node_count: number;
  edge_count: number;
  file_count: number;
  languages: LangStat[];
  last_indexed_at: number;
  unresolved_ref_count?: number | null;
  db_size_bytes: number;
  policy_rules_count: number;
  policy_skills_count: number;
  readonly: boolean;
  project_name: string;
}

export interface NodeRow {
  id: string;
  kind: string;
  name: string;
  qualified_name: string;
  file_path: string;
  language: string;
  start_line: number;
  end_line: number;
  signature: string | null;
  is_exported: number;
}

export interface EdgeNode {
  id: string;
  kind: string;
  name: string;
  file_path: string;
  start_line: number;
  edge_kind: string;
}

export interface NodeDetailRow {
  id: string;
  kind: string;
  name: string;
  qualified_name: string;
  file_path: string;
  language: string;
  start_line: number;
  end_line: number;
  signature: string | null;
  docstring: string | null;
  visibility: string | null;
  is_exported: number;
  is_async: number;
}

export interface NodeDetail {
  node: NodeDetailRow;
  callers: EdgeNode[];
  callees: EdgeNode[];
}

export interface FileRow {
  path: string;
  language: string;
  size: number;
  node_count: number;
  indexed_at: number;
}

export interface SearchResult {
  id: string;
  kind: string;
  name: string;
  qualified_name: string;
  file_path: string;
  start_line: number;
  language: string;
  snippet: string | null;
}
