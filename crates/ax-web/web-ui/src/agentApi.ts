const BASE = '/api/agent';

export interface AgentTargetStatus {
  id: string;
  display_name: string;
  bin?: string;
  detected: boolean;
  cli_available?: boolean;
  cli_on_path: boolean;
  data_dir_detected?: boolean;
  runnable?: boolean;
  cli_installable: boolean;
  configured: boolean;
  config_paths: string[];
}

export function isCliReady(t: AgentTargetStatus): boolean {
  return t.cli_available ?? t.cli_on_path;
}

/** All external agents that can run headless prompts in the agent terminal. */
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

export function runnableAgents(catalog: AgentTargetStatus[]): AgentTargetStatus[] {
  const byId = new Map(catalog.map((t) => [t.id, t]));
  return TERMINAL_EXTERNAL_AGENTS.filter((a) => {
    const row = byId.get(a.id);
    return row?.runnable !== false && !MCP_ONLY_AGENTS.has(a.id);
  }).map((a) => {
    const row = byId.get(a.id);
    return (
      row ?? {
        id: a.id,
        display_name: a.label,
        detected: false,
        cli_on_path: false,
        cli_installable: true,
        configured: false,
        config_paths: [],
        runnable: true,
      }
    );
  });
}

export function isRunnableAgent(agentId: string, catalog: AgentTargetStatus[]): boolean {
  if (agentId === 'builtin') return true;
  if (MCP_ONLY_AGENTS.has(agentId)) return false;
  const row = catalog.find((t) => t.id === agentId);
  if (row?.runnable === false) return false;
  return TERMINAL_EXTERNAL_AGENTS.some((a) => a.id === agentId);
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

export async function streamAgentCliInstall(
  targets: string[],
  onEvent: (ev: AgentStreamEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const res = await fetch(`${BASE}/cli/install/stream`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ targets }),
    signal,
  });
  if (!res.ok || !res.body) {
    onEvent({ type: 'done', ok: false, error: `HTTP ${res.status}` });
    return;
  }
  await readSse(res.body, onEvent);
}

export async function installAgents(targets: string[]): Promise<{ ok: boolean; error?: string }> {
  const res = await fetch(`${BASE}/install`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ targets }),
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

export async function streamAgentInstall(
  targets: string[],
  onEvent: (ev: AgentStreamEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const res = await fetch(`${BASE}/install/stream`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ targets }),
    signal,
  });
  if (!res.ok || !res.body) {
    onEvent({ type: 'done', ok: false, error: `HTTP ${res.status}` });
    return;
  }
  await readSse(res.body, onEvent);
}

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
  await readSse(res.body, onEvent);
}

export async function createAgentProfile(body: {
  agent: string;
  id: string;
  label: string;
  provider?: string;
  key_env?: string;
  model?: string;
}): Promise<{ ok: boolean; profile?: ProfileEntry; error?: string }> {
  const res = await fetch(`${BASE}/profiles`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json();
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

const CLI_INSTALL_TIMEOUT_MS = 240_000;
const MCP_WIRING_TIMEOUT_MS = 120_000;

/** Install CLI and wire MCP when the selected external agent is not ready. */
export async function ensureAgentReady(
  agentId: string,
  onLog: (text: string) => void,
  opts?: { signal?: AbortSignal; quiet?: boolean; skipInstall?: boolean },
): Promise<{ ok: boolean; error?: string; aborted?: boolean }> {
  if (agentId === 'builtin') return { ok: true };
  const signal = opts?.signal;
  const quiet = opts?.quiet ?? false;
  const skipInstall = opts?.skipInstall ?? false;

  if (signal?.aborted) return { ok: false, aborted: true, error: 'Setup cancelled' };

  let status = await fetchAgentStatus();
  if (signal?.aborted) return { ok: false, aborted: true, error: 'Setup cancelled' };

  const catalog = status.catalog ?? status.targets ?? [];
  let target = catalog.find((t) => t.id === agentId);

  if (!isRunnableAgent(agentId, catalog)) {
    const label = target?.display_name ?? TERMINAL_EXTERNAL_AGENTS.find((a) => a.id === agentId)?.label ?? agentId;
    return { ok: false, error: `${label} is MCP-only — not available in the agent terminal` };
  }

  const displayName =
    target?.display_name ?? TERMINAL_EXTERNAL_AGENTS.find((a) => a.id === agentId)?.label ?? agentId;
  const needsCli = !target || target.cli_installable !== false;
  const cliReady = target ? isCliReady(target) : false;

  if (needsCli && !cliReady && skipInstall) {
    return { ok: true };
  }

  if (needsCli && !cliReady) {
    let installErr: string | undefined;
    let aborted = false;
    const ac = new AbortController();
    await new Promise<void>((resolve) => {
      let settled = false;
      const finish = () => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        signal?.removeEventListener('abort', onParentAbort);
        resolve();
      };
      const onParentAbort = () => {
        aborted = true;
        ac.abort();
        finish();
      };
      signal?.addEventListener('abort', onParentAbort);
      const timer = setTimeout(() => {
        ac.abort();
        if (!installErr) {
          installErr =
            'CLI install timed out — install manually in Settings → AI Agents, then retry';
        }
        finish();
      }, CLI_INSTALL_TIMEOUT_MS);
      void streamAgentCliInstall(
        [agentId],
        (ev) => {
          if (signal?.aborted || aborted) return;
          if (ev.type === 'line') onLog(ev.text);
          if (ev.type === 'done') {
            if (ev.ok === false) installErr = ev.error ?? 'CLI install failed';
            finish();
          }
        },
        ac.signal,
      ).catch(() => {
        if (!installErr && !aborted) installErr = 'CLI install failed';
        finish();
      });
      if (signal?.aborted) onParentAbort();
    });
    if (signal?.aborted || aborted) return { ok: false, aborted: true, error: 'Setup cancelled' };
    if (installErr) return { ok: false, error: installErr };
  }

  status = await fetchAgentStatus();
  if (signal?.aborted) return { ok: false, aborted: true, error: 'Setup cancelled' };

  target = (status.catalog ?? status.targets ?? []).find((t) => t.id === agentId);
  if (needsCli && target && !isCliReady(target)) {
    return {
      ok: false,
      error: `${displayName} CLI not available after install — try: Settings → AI Agents → Install CLI`,
    };
  }
  if (target && !target.configured) {
    if (!quiet) onLog(`Wiring ax MCP for ${target.display_name}…`);
    let mcpErr: string | undefined;
    let mcpAborted = false;
    await new Promise<void>((resolve) => {
      let settled = false;
      const finish = () => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        signal?.removeEventListener('abort', onParentAbort);
        resolve();
      };
      const onParentAbort = () => {
        mcpAborted = true;
        ac.abort();
        finish();
      };
      const ac = new AbortController();
      signal?.addEventListener('abort', onParentAbort);
      const timer = setTimeout(() => {
        ac.abort();
        if (!mcpErr) {
          mcpErr = 'MCP wiring timed out — retry from Settings → AI Agents';
        }
        finish();
      }, MCP_WIRING_TIMEOUT_MS);
      void streamAgentInstall(
        [agentId],
        (ev) => {
          if (signal?.aborted || mcpAborted) return;
          if (ev.type === 'line') onLog(ev.text);
          if (ev.type === 'done') {
            if (ev.ok === false) mcpErr = ev.error ?? 'MCP wiring failed';
            finish();
          }
        },
        ac.signal,
      ).catch(() => {
        if (!mcpErr && !mcpAborted) mcpErr = 'MCP wiring failed';
        finish();
      });
      if (signal?.aborted) onParentAbort();
    });
    if (signal?.aborted || mcpAborted) return { ok: false, aborted: true, error: 'Setup cancelled' };
    if (mcpErr) return { ok: false, error: mcpErr };
  }

  return { ok: true };
}

export async function markProfileAuthenticated(
  agent: string,
  profileId: string,
): Promise<{ ok: boolean; error?: string }> {
  const res = await fetch(
    `${BASE}/profiles/${encodeURIComponent(agent)}/${encodeURIComponent(profileId)}/authenticated`,
    { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' },
  );
  return res.json();
}

export async function streamProfileAuth(
  agent: string,
  profileId: string,
  onEvent: (ev: AgentStreamEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const res = await fetch(`${BASE}/profiles/${encodeURIComponent(agent)}/${encodeURIComponent(profileId)}/auth/stream`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: '{}',
    signal,
  });
  if (!res.ok || !res.body) {
    onEvent({ type: 'done', ok: false, error: `HTTP ${res.status}` });
    return;
  }
  await readSse(res.body, onEvent);
}

async function readSse(body: ReadableStream<Uint8Array>, onEvent: (ev: AgentStreamEvent) => void) {
  const reader = body.getReader();
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
        if (line.startsWith('data: ')) {
          try {
            onEvent(JSON.parse(line.slice(6)) as AgentStreamEvent);
          } catch {
            /* ignore */
          }
        }
      }
    }
  }
}
