export type TraceLike = {
  sessionId?: string | null;
  turnId?: string | null;
};

export type TurnGroup<T> = {
  turnId: string;
  events: T[];
};

export type SessionGroup<T> = {
  sessionId: string;
  turns: TurnGroup<T>[];
};

/** Group flat trace rows as session → turn → events. Blank ids become Unlinked. */
export function groupBySessionThenTurn<T extends TraceLike>(
  events: T[],
): SessionGroup<T>[] {
  const sessions = new Map<string, Map<string, T[]>>();
  for (const event of events) {
    const sessionId = (event.sessionId ?? "").trim() || "Unlinked";
    const turnId = (event.turnId ?? "").trim() || "Unlinked";
    let turns = sessions.get(sessionId);
    if (!turns) {
      turns = new Map();
      sessions.set(sessionId, turns);
    }
    const bucket = turns.get(turnId) ?? [];
    bucket.push(event);
    turns.set(turnId, bucket);
  }
  return [...sessions.entries()].map(([sessionId, turns]) => ({
    sessionId,
    turns: [...turns.entries()].map(([turnId, events]) => ({ turnId, events })),
  }));
}
