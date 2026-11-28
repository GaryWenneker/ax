export const PTY_MAX_RETRIES = 5;
/** A connection must stay open this long before the retry count starts over. */
export const PTY_STABLE_MS = 5000;

export type PtyLink = { retries: number; openedAt: number | null; serverError: boolean };
export type PtyNext =
  | { kind: 'retry'; attempt: number; delay: number }
  | { kind: 'stop'; reason: 'server-error' | 'exhausted' };

export function ptyLink(): PtyLink {
  return { retries: 0, openedAt: null, serverError: false };
}

export function ptyOpened(link: PtyLink, now: number): PtyLink {
  return { ...link, openedAt: now, serverError: false };
}

/** The server reported an error (spawn failure, missing CLI, refused profile); it closes the socket next. */
export function ptyServerError(link: PtyLink): PtyLink {
  return { ...link, serverError: true };
}

/** Decide what to do after the socket closed at `now`. */
export function ptyClosed(link: PtyLink, now: number): { link: PtyLink; next: PtyNext } {
  if (link.serverError) return { link: { ...link, openedAt: null }, next: { kind: 'stop', reason: 'server-error' } };
  const stable = link.openedAt != null && now - link.openedAt >= PTY_STABLE_MS;
  const retries = stable ? 0 : link.retries;
  if (retries >= PTY_MAX_RETRIES) return { link: { ...link, retries, openedAt: null }, next: { kind: 'stop', reason: 'exhausted' } };
  const attempt = retries + 1;
  const delay = Math.min(1000 * 2 ** (attempt - 1), 10_000);
  return { link: { ...link, retries: attempt, openedAt: null }, next: { kind: 'retry', attempt, delay } };
}
