/** Helpers for Settings → Remote policy share. */

export function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/**
 * Polls every `intervalMs`, but only schedules the next poll after the previous
 * one settled, so a slow link never has two polls in flight. `tick` resolves
 * true when polling is done; a rejected tick also ends polling.
 */
export function startSerialPoll(opts: {
  intervalMs: number;
  tick: () => Promise<boolean>;
  setTimer?: (cb: () => void, ms: number) => number;
  clearTimer?: (id: number) => void;
}): { stop: () => void } {
  const setTimer = opts.setTimer ?? ((cb, ms) => window.setTimeout(cb, ms));
  const clearTimer = opts.clearTimer ?? ((id) => window.clearTimeout(id));
  let stopped = false;
  let timer: number | null = null;
  const schedule = () => {
    timer = setTimer(() => {
      timer = null;
      if (stopped) return;
      opts.tick().then(
        (done) => {
          if (done) stopped = true;
          if (!stopped) schedule();
        },
        () => {
          stopped = true;
        },
      );
    }, opts.intervalMs);
  };
  schedule();
  return {
    stop: () => {
      stopped = true;
      if (timer !== null) clearTimer(timer);
      timer = null;
    },
  };
}

/** Runs a save; a failure goes to `onError` and resolves false instead of rejecting. */
export async function saveQuietly(save: () => Promise<unknown>, onError: (message: string) => void): Promise<boolean> {
  try {
    await save();
    return true;
  } catch (e) {
    onError(errorText(e));
    return false;
  }
}

/**
 * Text fields save on blur. Keeping them enabled during that save stops the
 * field that just took focus from being disabled under the cursor. Toggles,
 * selects and buttons stay disabled while anything runs.
 */
export function shareFieldDisabled(state: { loaded: boolean; busy: string | null; kind: 'text' | 'control' }): boolean {
  if (!state.loaded) return true;
  if (!state.busy) return false;
  return !(state.kind === 'text' && state.busy === 'save');
}
