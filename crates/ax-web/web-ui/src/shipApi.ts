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
}

export interface ShipConfig {
  ship: { target_branch: string; web_port: number };
  quality_gate: { steps: string[]; tests: { runner: string } };
  remote: {
    provider: string;
    github?: { owner: string; repo: string; token_env: string } | null;
    azure_devops?: { org: string; project: string; repo_id: string; token_env: string } | null;
  };
  sonar: SonarConfig;
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
  reachable: boolean;
  host: string;
}

export interface SonarSetupStatus {
  login_user: string;
  login_password_hint: string;
  project_exists: boolean;
  token_configured: boolean;
  scanner_available: boolean;
  token_path: string;
}

export interface SonarBootstrapResult {
  project_created: boolean;
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

export function fetchShipStatus(): Promise<{ branch: string | null; report: ShipReport | null; config: ShipConfig }> {
  return request('/status');
}

export function fetchShipConfig(): Promise<{
  config: ShipConfig;
  sonar: SonarDiscovery;
  sonar_setup?: SonarSetupStatus;
}> {
  return request('/config');
}

export function saveShipConfig(config: ShipConfig): Promise<{ ok: boolean }> {
  return request('/config', { method: 'PUT', body: JSON.stringify(config) });
}

export function discoverSonar(): Promise<{ discovery: SonarDiscovery; setup?: SonarSetupStatus }> {
  return request('/sonar/discover');
}

export function bootstrapSonar(): Promise<{
  ok: boolean;
  result?: SonarBootstrapResult;
  setup?: SonarSetupStatus;
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
  | { type: 'done'; ok: boolean; discovery?: SonarDiscovery; error?: string; logs?: string[] };

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

export function runShipCommand(
  cmd: string,
  opts?: { title?: string; body?: string },
): Promise<{ ok: boolean; report?: ShipReport; pr?: { number: number; url: string }; error?: string }> {
  return request('/command', { method: 'POST', body: JSON.stringify({ cmd, ...opts }) });
}
