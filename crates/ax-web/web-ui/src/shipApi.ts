const SHIP = '/api/ship';

export interface GateStep {
  step: string;
  status: string;
  detail?: string;
}

export interface ShipReport {
  git?: { head_branch?: string; base_ref: string };
  changed_files?: string[];
  tia?: { tests: Array<{ name: string; runner_hint: string }>; test_files: string[] };
  quality_gate?: {
    passed: boolean;
    steps: GateStep[];
    sonar?: { status: string; passed: boolean };
  };
  breaking_warnings?: Array<{ node_name: string; reason: string }>;
  business_rule_warnings?: Array<{ rule_text: string; severity: string }>;
  affected_routes?: string[];
}

export interface SonarConfig {
  enabled: boolean;
  host: string;
  project_key: string;
  token_env: string;
  scanner_path: string;
  podman_container?: string | null;
  container_runtime: string;
  scan_mode?: string;
  admin_user?: string;
  admin_password?: string;
  exclude_repos?: string[];
}

export interface UiConfig {
  show_savings?: boolean;
  /** @deprecated use show_savings */
  show_tokens?: boolean;
  show_agent_terminal?: boolean;
  /** Emit MCP inbound/outbound/enrichment traces to Cursor MCP Output (stderr). */
  verbose_mcp?: boolean;
}

export interface ShipConfig {
  ship: { target_branch: string; web_port: number; git_root?: string | null; git_roots?: string[] };
  quality_gate: { steps: string[]; tests: { runner: string }; index_mode?: string };
  remote: {
    provider: string;
    github?: { owner: string; repo: string; token_env: string } | null;
    azure_devops?: { org: string; project: string; repo_id: string; token_env: string } | null;
  };
  sonar: SonarConfig;
  ui?: UiConfig;
  reviewers: Record<string, string>;
}

export interface RuntimeInfo {
  runtime: 'podman' | 'docker';
  version: string;
  available: boolean;
}

export interface ContainerInfo {
  name: string;
  runtime: 'podman' | 'docker';
  status: string;
  running: boolean;
  ports: string;
}

export interface SonarDiscovery {
  runtimes: RuntimeInfo[];
  preferred: 'podman' | 'docker' | null;
  container: ContainerInfo | null;
  database?: ContainerInfo | null;
  reachable: boolean;
  host: string;
  embedded_database?: boolean;
}

export interface RepoProjectStatus {
  key: string;
  name: string;
  exists: boolean;
}

export interface SonarSetupStatus {
  login_user: string;
  login_password_hint: string;
  project_exists: boolean;
  project_lookup: 'found' | 'missing' | 'auth_failed' | 'unreachable';
  repo_projects?: RepoProjectStatus[];
  token_configured: boolean;
  token_valid?: boolean | null;
  scanner_available: boolean;
  token_path: string;
}

export interface LastRunLog {
  started_at?: string | null;
  finished_at?: string | null;
  ok: boolean;
  lines: string[];
}

export interface SonarBootstrapResult {
  project_created: boolean;
  project_key: string;
  project_name: string;
  projects_created?: number;
  repo_projects?: RepoProjectStatus[];
  token_saved: boolean;
  token_env_set: boolean;
  ui_url: string;
  login_user: string;
  login_password_hint: string;
  token_path: string;
  message: string;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${SHIP}${path}`, {
    headers: { 'Content-Type': 'application/json', ...(init?.headers ?? {}) },
    ...init,
  });
  const body = await res.json().catch(() => ({}));
  if (!res.ok) {
    throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
  }
  return body as T;
}

export function fetchShipStatus(): Promise<{
  branch: string | null;
  report: ShipReport | null;
  config: ShipConfig;
  last_run?: LastRunLog;
  evaluating?: boolean;
}> {
  return request('/status');
}

export function fetchShipConfig(): Promise<{
  config: ShipConfig;
  sonar: SonarDiscovery;
  sonar_setup?: SonarSetupStatus;
  git_roots_discovered?: string[];
}> {
  return request('/config');
}

export function saveShipConfig(config: ShipConfig): Promise<{ ok: boolean }> {
  return request('/config', { method: 'PUT', body: JSON.stringify(config) });
}

export const SONAR_UI_PROXY = '/api/ship/sonar/ui/';

export interface SonarUiInfo {
  ok: boolean;
  reachable?: boolean;
  proxy_url?: string;
  host?: string;
  dark_mode?: string;
  error?: string;
}

export function fetchSonarUiInfo(): Promise<SonarUiInfo> {
  return request('/sonar/ui/info');
}

export function discoverSonar(): Promise<{ discovery: SonarDiscovery; setup?: SonarSetupStatus }> {
  return request('/sonar/discover');
}

export function fetchSonarSetup(): Promise<{ ok: boolean; setup?: SonarSetupStatus; error?: string }> {
  return request('/sonar/setup');
}

export interface SonarTokenCheck {
  ok: boolean;
  reachable?: boolean;
  configured?: boolean;
  valid?: boolean;
  message?: string;
  error?: string;
}

export function validateSonarToken(): Promise<SonarTokenCheck> {
  return request('/sonar/validate-token');
}

export function regenerateSonarToken(): Promise<{
  ok: boolean;
  result?: SonarBootstrapResult;
  setup?: SonarSetupStatus;
  error?: string;
}> {
  return request('/sonar/regenerate-token', { method: 'POST', body: '{}' });
}

export function bootstrapSonar(): Promise<{
  ok: boolean;
  result?: SonarBootstrapResult;
  setup?: SonarSetupStatus;
  config?: ShipConfig;
  error?: string;
}> {
  return request('/sonar/bootstrap', { method: 'POST', body: '{}' });
}

export function installSonar(): Promise<{
  ok: boolean;
  discovery?: SonarDiscovery;
  error?: string;
  logs?: string[];
}> {
  return request('/sonar/install', { method: 'POST', body: '{}' });
}

export function startSonar(): Promise<{
  ok: boolean;
  discovery?: SonarDiscovery;
  error?: string;
  logs?: string[];
}> {
  return request('/sonar/start', { method: 'POST', body: '{}' });
}

export type SonarStreamEvent =
  | { type: 'log'; line: string }
  | {
      type: 'done';
      ok: boolean;
      discovery?: SonarDiscovery;
      quality_gate?: { status: string; passed: boolean };
      error?: string;
      logs?: string[];
    };

async function consumeSse(
  body: ReadableStream<Uint8Array>,
  onEvent: (ev: SonarStreamEvent) => void,
): Promise<void> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const chunks = buffer.split('\n\n');
    buffer = chunks.pop() ?? '';
    for (const chunk of chunks) {
      for (const line of chunk.split('\n')) {
        if (!line.startsWith('data: ')) continue;
        try {
          onEvent(JSON.parse(line.slice(6)) as SonarStreamEvent);
        } catch {
          /* ignore malformed */
        }
      }
    }
  }
}

export async function streamSonarInstall(
  onEvent: (ev: SonarStreamEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const res = await fetch(`${SHIP}/sonar/install/stream`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: '{}',
    signal,
  });
  if (!res.ok || !res.body) {
    const body = await res.json().catch(() => ({}));
    throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
  }
  await consumeSse(res.body, onEvent);
}

export async function streamSonarStart(
  onEvent: (ev: SonarStreamEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const res = await fetch(`${SHIP}/sonar/start/stream`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: '{}',
    signal,
  });
  if (!res.ok || !res.body) {
    const body = await res.json().catch(() => ({}));
    throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
  }
  await consumeSse(res.body, onEvent);
}

export async function streamSonarStop(
  onEvent: (ev: SonarStreamEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const res = await fetch(`${SHIP}/sonar/stop/stream`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: '{}',
    signal,
  });
  if (!res.ok || !res.body) {
    const body = await res.json().catch(() => ({}));
    throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
  }
  await consumeSse(res.body, onEvent);
}

export async function streamSonarScanAll(
  onEvent: (ev: SonarStreamEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  return streamSonarScan(onEvent, signal);
}

export async function streamSonarScanProject(
  projectKey: string,
  onEvent: (ev: SonarStreamEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  return streamSonarScan(onEvent, signal, projectKey);
}

async function streamSonarScan(
  onEvent: (ev: SonarStreamEvent) => void,
  signal?: AbortSignal,
  projectKey?: string,
): Promise<void> {
  const body = projectKey ? JSON.stringify({ project_key: projectKey }) : '{}';
  const res = await fetch(`${SHIP}/sonar/scan/stream`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body,
    signal,
  });
  if (!res.ok || !res.body) {
    const body = await res.json().catch(() => ({}));
    throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
  }
  await consumeSse(res.body, onEvent);
}

export function toggleSonarExclude(
  repoName: string,
  excluded: boolean,
): Promise<{ ok: boolean; exclude_repos: string[] }> {
  return request('/sonar/exclude', {
    method: 'POST',
    body: JSON.stringify({ repo: repoName, excluded }),
  });
}

export function runShipCommand(
  cmd: string,
  opts?: { title?: string; body?: string },
): Promise<{
  ok: boolean;
  started?: boolean;
  evaluating?: boolean;
  report?: ShipReport;
  pr?: { number: number; url: string };
  error?: string;
  last_run?: LastRunLog;
}> {
  return request('/command', { method: 'POST', body: JSON.stringify({ cmd, ...opts }) });
}
