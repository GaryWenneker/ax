export function linkedSessions(sessions: string): string[] {
  const ids = sessions.split(/\s+/).filter(Boolean);
  const real = ids.filter((id) => !id.startsWith('plan-'));
  return real.length > 0 ? real : ids;
}

export function isChatSession(id: string): boolean {
  const value = id.trim();
  return value !== '' && value !== 'sync' && value !== 'Unlinked';
}

export function specForSession<T extends { sessions: string }>(
  specs: readonly T[],
  session: string,
): T | undefined {
  const needle = session.trim();
  if (!needle) return undefined;
  return specs.find((spec) => spec.sessions.split(/\s+/).includes(needle));
}

/** One button in the session hop: where it goes, in route terms. */
export type SessionHopTarget =
  | { label: 'Logging'; route: { page: 'logging'; session: string } }
  | { label: 'Memory'; route: { page: 'memory'; session: string } }
  | { label: 'Plan'; route: { page: 'specs'; specId: string } };

/** Logging, Memory, and the session's plan; the current page's own button is left out. */
export function sessionHopTargets(
  session: string,
  here: 'memory' | 'logging',
  plan: { id: string } | null,
): SessionHopTarget[] {
  if (!isChatSession(session)) return [];
  const targets: SessionHopTarget[] = [];
  if (here !== 'logging') targets.push({ label: 'Logging', route: { page: 'logging', session } });
  if (here !== 'memory') targets.push({ label: 'Memory', route: { page: 'memory', session } });
  if (plan) targets.push({ label: 'Plan', route: { page: 'specs', specId: plan.id } });
  return targets;
}
