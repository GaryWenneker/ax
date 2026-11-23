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
