export const DEFAULT_BASE_URL = "http://127.0.0.1:7070";

export interface GetaxSettings {
  baseUrl: string;
}

export interface VersionResponse {
  version: string;
}

export interface SearchHit {
  id: string;
  kind: string;
  name: string;
  qualified_name: string;
  file_path: string;
  start_line: number;
  language: string;
  snippet?: string | null;
}

export interface PolicyRule {
  id: string;
  level: string;
  body: string;
  tags?: string[];
  enabled?: boolean;
}

export interface PolicySkill {
  name: string;
  description: string;
  body: string;
  tags?: string[];
  enabled?: boolean;
}

export interface MemoryRow {
  id: string;
  kind: string;
  title: string;
  body: string;
  tags?: string[];
}

export interface MemoryMatch {
  id: string;
  kind: string;
  title: string;
  body: string;
  score?: number;
}

export interface GraphNode {
  id: string;
  name: string;
  kind: string;
  file_path: string;
  community_id: number;
  degree: number;
  selected?: boolean;
}

export interface GraphEdge {
  source: string;
  target: string;
  kind: string;
}

export interface GraphPayload {
  nodes: GraphNode[];
  edges: GraphEdge[];
  total_nodes: number;
  truncated: boolean;
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
  signature?: string | null;
  docstring?: string | null;
}

export interface EdgeNode {
  id: string;
  kind: string;
  name: string;
  file_path: string;
  start_line: number;
  edge_kind: string;
}

export interface NodeDetail {
  node: NodeDetailRow;
  callers?: EdgeNode[];
  callees?: EdgeNode[];
}

export type ResultKind = "code" | "rule" | "skill" | "memory";

export interface DisplayHit {
  kind: ResultKind;
  title: string;
  subtitle: string;
  body: string;
}
