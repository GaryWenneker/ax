import { readSseJson } from './lib/sse.ts';

const BASE = '/api/agent';

export interface AgentTargetStatus {
  id: string;
  display_name: string;
  bin?: string;
  detected: boolean;
  cli_available?: boolean;
  cli_on_path: boolean;
  app_installed?: boolean;
  runnable?: boolean;
  cli_installable: boolean;
  configured: boolean;
  config_paths: string[];
  /** Command Center present in the IDE; absent or null for terminal agents. */
  panel?: boolean | null;
}

export function isCliReady(t: AgentTargetStatus): boolean {
  return t.cli_available ?? t.cli_on_path;
}

/** All external agents that can run headless prompts. */
export const TERMINAL_EXTERNAL_AGENTS: { id: string; label: string }[] = [
  { id: 'claude', label: 'Claude Code' },
  { id: 'cursor', label: 'Cursor' },
  { id: 'codex', label: 'Codex CLI' },
  { id: 'gemini', label: 'Gemini CLI' },
  { id: 'opencode', label: 'opencode' },
  { id: 'kiro', label: 'Kiro' },
];

const MCP_ONLY_AGENTS = new Set(['hermes', 'antigravity']);

export function terminalAgentOptions(catalog: AgentTargetStatus[]): { id: string; label: string }[] {
  const byId = new Map(catalog.map((t) => [t.id, t]));
  const external = TERMINAL_EXTERNAL_AGENTS.filter((a) => {
    const row = byId.get(a.id);
    if (row?.runnable === false) return false;
    if (MCP_ONLY_AGENTS.has(a.id)) return false;
    return true;
  }).map((a) => ({
    id: a.id,
    label: byId.get(a.id)?.display_name ?? a.label,
  }));
  return [{ id: 'builtin', label: 'Built-in ax' }, ...external];
}

export interface ProfileEntry {
  id: string;
  label: string;
  data_dir: string;
  auth_status: 'authenticated' | 'needs_auth' | 'unknown';
  provider?: string;
  key_env?: string;
  model?: string;
}

export interface AgentsConfig {
  preferred_external?: string;
  last_terminal_agent?: string;
  enabled_targets: string[];
  terminal_mode: string;
  active_profile: Record<string, string>;
  profiles: Record<string, ProfileEntry[]>;
}

export async function fetchAgentStatus(): Promise<{
  ok: boolean;
  readonly?: boolean;
  targets: AgentTargetStatus[];
  catalog?: AgentTargetStatus[];
  config: AgentsConfig;
  all_targets: string[];
}> {
  const res = await fetch(`${BASE}/status`);
  const data = await res.json();
  if (data.catalog && !data.targets?.length) {
    data.targets = data.catalog;
  }
  return data;
}

export async function saveAgentConfig(config: AgentsConfig): Promise<{ ok: boolean; error?: string }> {
  const res = await fetch(`${BASE}/config`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ config }),
  });
  return res.json();
}

export type AgentStreamEvent =
  | { type: 'line'; text: string }
  | { type: 'token'; text: string }
  | { type: 'system'; text: string }
  | { type: 'tool_start'; name: string }
  | { type: 'tool_end'; name: string; preview: string }
  | { type: 'error'; message: string }
  | { type: 'done'; ok?: boolean; session_id?: string; error?: string; manual?: boolean };

export async function streamAgentChat(
  prompt: string,
  opts: { sessionId?: string; agent?: string; profileId?: string },
  onEvent: (ev: AgentStreamEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const res = await fetch(`${BASE}/chat/stream`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      prompt,
      session_id: opts.sessionId,
      agent: opts.agent,
      profile_id: opts.profileId,
    }),
    signal,
  });
  if (!res.ok || !res.body) {
    onEvent({ type: 'error', message: `HTTP ${res.status}` });
    onEvent({ type: 'done' });
    return;
  }
  await readSseJson<AgentStreamEvent>(res.body, onEvent);
}

export async function updateAgentProfile(
  agent: string,
  id: string,
  body: { label?: string; provider?: string; key_env?: string; model?: string },
): Promise<{ ok: boolean; profile?: ProfileEntry; error?: string }> {
  const res = await fetch(`${BASE}/profiles/${encodeURIComponent(agent)}/${encodeURIComponent(id)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json();
}

export async function setActiveProfile(agent: string, profileId: string): Promise<{ ok: boolean; error?: string }> {
  const res = await fetch(`${BASE}/profiles/active`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ agent, profile_id: profileId }),
  });
  return res.json();
}
