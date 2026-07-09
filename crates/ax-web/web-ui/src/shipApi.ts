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

export function fetchShipConfig(): Promise<{ config: ShipConfig; sonar: SonarDiscovery }> {
  return request('/config');
}

export function saveShipConfig(config: ShipConfig): Promise<{ ok: boolean }> {
  return request('/config', { method: 'PUT', body: JSON.stringify(config) });
}

export function discoverSonar(): Promise<{ discovery: SonarDiscovery }> {
  return request('/sonar/discover');
}

export function installSonar(): Promise<{ ok: boolean; discovery?: SonarDiscovery; error?: string }> {
  return request('/sonar/install', { method: 'POST', body: '{}' });
}

export function startSonar(): Promise<{ ok: boolean; discovery?: SonarDiscovery; error?: string }> {
  return request('/sonar/start', { method: 'POST', body: '{}' });
}

export function runShipCommand(
  cmd: string,
  opts?: { title?: string; body?: string },
): Promise<{ ok: boolean; report?: ShipReport; pr?: { number: number; url: string }; error?: string }> {
  return request('/command', { method: 'POST', body: JSON.stringify({ cmd, ...opts }) });
}
