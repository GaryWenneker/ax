/** Pull session/turn ids from a Logging line body (`session=` / `turn=`). */
export function traceIdsFromMessage(message: string): {
  sessionId: string | null;
  turnId: string | null;
} {
  const session = message.match(/\bsession=(\S+)/)?.[1]?.trim() || null;
  const turn = message.match(/\bturn=(\S+)/)?.[1]?.trim() || null;
  return { sessionId: session, turnId: turn };
}
