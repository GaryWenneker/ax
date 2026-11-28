/** Helpers for pages that reload the same data on demand and on live events. */

/** Numbers each request so a slow, older response cannot overwrite a newer one. */
export interface LatestGate {
  /** Start a request and get its ticket; every earlier ticket stops being the latest. */
  begin: () => number;
  /** Whether no newer request began (and nothing cancelled) since this ticket. */
  isLatest: (ticket: number) => boolean;
  /** Drop every request in flight, for example when the input is cleared or the selection closed. */
  cancel: () => void;
}

export function createLatestGate(): LatestGate {
  let latest = 0;
  return {
    begin: () => {
      latest += 1;
      return latest;
    },
    isLatest: (ticket) => ticket === latest,
    cancel: () => {
      latest += 1;
    },
  };
}

export interface LatestHandlers<T> {
  value: (value: T) => void;
  error?: (error: unknown) => void;
  /** Runs after `value` or `error`, only for the latest request. */
  settled?: () => void;
}

/**
 * Run `load` as a new request on `gate`. Its value or error reaches the
 * handlers only when no newer request began and the gate was not cancelled
 * in the meantime. Resolves to whether the answer was applied.
 */
export async function applyLatest<T>(gate: LatestGate, load: () => Promise<T>, on: LatestHandlers<T>): Promise<boolean> {
  const ticket = gate.begin();
  let result: { ok: true; value: T } | { ok: false; error: unknown };
  try {
    result = { ok: true, value: await load() };
  } catch (error) {
    result = { ok: false, error };
  }
  if (!gate.isLatest(ticket)) return false;
  if (result.ok) on.value(result.value);
  else on.error?.(result.error);
  on.settled?.();
  return true;
}

/**
 * The data to show after a load fails. A quiet (background) refresh keeps what
 * is on screen; a load the user asked for (a new period) clears it so old
 * numbers are not shown under the new choice.
 */
export function dataAfterFailedLoad<T>(shown: T | null, quiet: boolean): T | null {
  return quiet ? shown : null;
}
