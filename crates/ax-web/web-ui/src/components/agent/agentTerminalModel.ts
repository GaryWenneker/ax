import { isCliReady, TERMINAL_EXTERNAL_AGENTS, type AgentTargetStatus, type AgentsConfig } from '../../agentApi.ts';

export function resolveInitialAgent(cfg: AgentsConfig): string {
  if (cfg.last_terminal_agent) return cfg.last_terminal_agent;
  if (cfg.terminal_mode === 'builtin') return 'builtin';
  return cfg.preferred_external ?? 'cursor';
}

export function profileForAgent(cfg: AgentsConfig | null, agent: string): string {
  if (!cfg) return 'default';
  return cfg.active_profile?.[agent] ?? cfg.profiles?.[agent]?.[0]?.id ?? 'default';
}

export function agentLabel(catalog: AgentTargetStatus[], id: string): string {
  return catalog.find((t) => t.id === id)?.display_name ?? TERMINAL_EXTERNAL_AGENTS.find((a) => a.id === id)?.label ?? id;
}

/** The agent, profile and first system line for a freshly loaded config and catalog. */
export function initialAgentPlan(
  cfg: AgentsConfig,
  catalog: AgentTargetStatus[],
): { agent: string; profileId: string; notice: string | null } {
  const agent = resolveInitialAgent(cfg);
  const profileId = profileForAgent(cfg, agent);
  if (agent === 'builtin') return { agent, profileId, notice: null };
  const target = catalog.find((t) => t.id === agent);
  const label = agentLabel(catalog, agent);
  const notice =
    target && !isCliReady(target) && target.cli_installable !== false
      ? `${label} CLI not installed — install it manually, then select ${label} again.`
      : `Interactive ${label} CLI · profile ${profileId}`;
  return { agent, profileId, notice };
}

/** The system line for a chat request that threw, or null when it was aborted on purpose. */
export function chatFailureLine(error: unknown): string | null {
  if (error instanceof Error && error.name === 'AbortError') return null;
  const message = error instanceof Error ? error.message : String(error);
  return `Chat request failed: ${message}`;
}

export function profileSwitchLine(res: { ok: boolean; error?: string }, id: string, agent: string): string {
  if (res.ok) return `Switched profile to ${id} (${agent})`;
  return res.error ? `Could not switch profile to ${id}: ${res.error}` : `Could not switch profile to ${id}`;
}
