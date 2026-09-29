import type {
  GraphPayload,
  MemoryMatch,
  NodeDetail,
  PolicyRule,
  PolicySkill,
  SearchHit,
  VersionResponse,
} from "./types";

function boundFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  return globalThis.fetch(input, init);
}

export class AxApi {
  private readonly baseUrl: string;
  private readonly fetchImpl: typeof fetch;

  constructor(baseUrl: string, fetchImpl: typeof fetch = boundFetch) {
    this.baseUrl = baseUrl;
    this.fetchImpl = fetchImpl;
  }

  root(): string {
    return this.baseUrl.replace(/\/+$/, "");
  }

  async version(): Promise<VersionResponse> {
    return this.get<VersionResponse>("/api/version");
  }

  async search(q: string): Promise<SearchHit[]> {
    const params = new URLSearchParams({ q, limit: "20" });
    const body = await this.get<{ results?: SearchHit[] }>(`/api/search?${params}`);
    return body.results ?? [];
  }

  async rules(): Promise<PolicyRule[]> {
    const body = await this.get<{ rules: PolicyRule[] }>("/api/policy/rules");
    return body.rules ?? [];
  }

  async skills(): Promise<PolicySkill[]> {
    const body = await this.get<{ skills: PolicySkill[] }>("/api/policy/skills");
    return body.skills ?? [];
  }

  async recall(q: string): Promise<MemoryMatch[]> {
    const params = new URLSearchParams({ q, limit: "20" });
    const body = await this.get<{ matches: MemoryMatch[] }>(`/api/memory/recall?${params}`);
    return body.matches ?? [];
  }

  async graph(): Promise<GraphPayload> {
    return this.get<GraphPayload>("/api/graph?limit=600");
  }

  async node(id: string): Promise<NodeDetail> {
    return this.get<NodeDetail>(`/api/node/${encodeURIComponent(id)}`);
  }

  async rule(id: string): Promise<PolicyRule> {
    return this.get<PolicyRule>(`/api/policy/rules/${encodeURIComponent(id)}`);
  }

  async skill(name: string): Promise<PolicySkill> {
    return this.get<PolicySkill>(`/api/policy/skills/${encodeURIComponent(name)}`);
  }

  async nodesInFile(file: string, kind: string): Promise<Array<{ id: string; name: string; kind: string; file_path: string }>> {
    const params = new URLSearchParams({ file, kind, limit: "2000" });
    const body = await this.get<{ nodes: Array<{ id: string; name: string; kind: string; file_path: string }> }>(
      `/api/nodes?${params}`,
    );
    return body.nodes ?? [];
  }
    async memories(): Promise<MemoryMatch[]> {
    const body = await this.get<{ memories: MemoryMatch[] }>("/api/memory/?limit=500");
    return body.memories ?? [];
  }

  async ruleDoc(id: string): Promise<{ frontmatter: Record<string, unknown>; body: string }> {
    return this.get(`/api/policy/rules/${encodeURIComponent(id)}`);
  }

  async skillDoc(name: string): Promise<{ frontmatter: Record<string, unknown>; body: string }> {
    return this.get(`/api/policy/skills/${encodeURIComponent(name)}`);
  }

  async saveRule(id: string | null, payload: unknown): Promise<void> {
    const body = JSON.stringify(payload);
    const headers = { "content-type": "application/json" };
    if (id) await this.send(`/api/policy/rules/${encodeURIComponent(id)}`, { method: "PUT", headers, body });
    else await this.send("/api/policy/rules", { method: "POST", headers, body });
  }

  async saveSkill(name: string | null, payload: unknown): Promise<void> {
    const body = JSON.stringify(payload);
    const headers = { "content-type": "application/json" };
    if (name) await this.send(`/api/policy/skills/${encodeURIComponent(name)}`, { method: "PUT", headers, body });
    else await this.send("/api/policy/skills", { method: "POST", headers, body });
  }

  async deleteRule(id: string): Promise<void> {
    await this.send(`/api/policy/rules/${encodeURIComponent(id)}`, { method: "DELETE" });
  }

  async deleteSkill(name: string): Promise<void> {
    await this.send(`/api/policy/skills/${encodeURIComponent(name)}`, { method: "DELETE" });
  }

  private async get<T>(path: string): Promise<T> {
    return this.send<T>(path);
  }

  async send<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await this.fetchImpl(`${this.root()}${path}`, init);
    if (!response.ok) {
      throw new Error(`${path} returned ${response.status}`);
    }
    const text = await response.text();
    if (!text) return undefined as T;
    return JSON.parse(text) as T;
  }
}
