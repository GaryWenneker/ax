/** Session checks for the Command Center sign-in gate. */

export type AuthMode = 'loading' | 'open' | 'login' | 'error';

export interface AuthState {
  mode: AuthMode;
  /** Error page text, or the banner shown while the app stays open. */
  notice: string | null;
  /** Transient failures in a row; drives the retry backoff. */
  failures: number;
}

export type SessionOutcome =
  | { kind: 'open'; origin: string | null }
  | { kind: 'login'; origin: string | null }
  | { kind: 'unavailable'; message: string };

async function readJson(response: Response): Promise<Record<string, unknown>> {
  try {
    return (await response.json()) as Record<string, unknown>;
  } catch {
    return {};
  }
}

/**
 * Ask the server whether sign-in is needed and whether this browser has a session.
 * Only a 401 from the session endpoint means "sign in"; a thrown fetch, a 5xx or
 * any other failure is reported as unavailable so callers can treat it as transient.
 */
export async function checkSession(fetchImpl: typeof fetch = fetch): Promise<SessionOutcome> {
  try {
    const statusRes = await fetchImpl('/api/web-auth/status', { credentials: 'same-origin', cache: 'no-store' });
    const status = await readJson(statusRes);
    if (!statusRes.ok) {
      const message = typeof status.error === 'string' ? status.error : `HTTP ${statusRes.status}`;
      return { kind: 'unavailable', message };
    }
    const origin = typeof status.origin === 'string' ? status.origin : null;
    if (!status.enabled) return { kind: 'open', origin };
    const session = await fetchImpl('/api/web-auth/session', { cache: 'no-store' });
    if (session.ok) return { kind: 'open', origin };
    if (session.status === 401) return { kind: 'login', origin };
    return { kind: 'unavailable', message: `Session check failed (HTTP ${session.status})` };
  } catch (e) {
    return { kind: 'unavailable', message: e instanceof Error ? e.message : 'Cannot check authentication' };
  }
}

/** Fold one session check into the gate state. */
export function nextAuthState(current: AuthState, outcome: SessionOutcome): AuthState {
  if (outcome.kind === 'open') return { mode: 'open', notice: null, failures: 0 };
  if (outcome.kind === 'login') return { mode: 'login', notice: null, failures: 0 };
  const failures = current.failures + 1;
  // Keep the app mounted while it is open: unsaved work below the gate must survive
  // a server restart or a network blip.
  if (current.mode === 'open') return { mode: 'open', notice: outcome.message, failures };
  return { mode: 'error', notice: outcome.message, failures };
}

/** Delay before the next automatic retry after `failures` transient failures in a row. */
export function retryDelayMs(failures: number): number {
  return Math.min(30_000, 1_000 * 2 ** Math.max(1, Math.min(failures, 15)));
}
