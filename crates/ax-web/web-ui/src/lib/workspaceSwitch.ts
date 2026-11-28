/** What the project pickers do with a workspace switch or mkdir reply. */

export interface SwitchResponse {
  ok: boolean;
  path?: string;
  reloading?: boolean;
  switched?: boolean;
  needs_init?: boolean;
  error?: string;
}

export type SwitchOutcome =
  | { kind: 'switched'; path?: string }
  | { kind: 'reload' }
  | { kind: 'needs_init'; path: string; message: string }
  | { kind: 'error'; message: string };

export const NEEDS_INIT_MESSAGE = 'Not an ax project yet — use Initialize below.';

/** `requested` is the folder the user asked to switch to; a needs-init reply browses there. */
export function switchOutcome(res: SwitchResponse, requested = ''): SwitchOutcome {
  if (res.needs_init) return { kind: 'needs_init', path: res.path || requested, message: NEEDS_INIT_MESSAGE };
  if (!res.ok) return { kind: 'error', message: res.error ?? 'Switch failed' };
  if (res.reloading && !res.switched) return { kind: 'reload' };
  return { kind: 'switched', path: res.path };
}

/** After a mkdir: the folder to browse next, or the error to show. */
export function mkdirOutcome(
  res: { ok: boolean; path?: string; error?: string },
  shown: string,
): { browse: string | null; error: string | null } {
  if (!res.ok) return { browse: null, error: res.error ?? 'Create failed' };
  return { browse: res.path ?? shown, error: null };
}
