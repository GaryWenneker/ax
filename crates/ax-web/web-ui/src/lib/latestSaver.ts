/**
 * Saves a value that changes quickly (a slider being dragged) without
 * flooding the server or letting an older write land last.
 *
 * `set` waits `delayMs` for the value to settle. At most one write is in
 * flight; a value set meanwhile is sent once that write finishes. Only the
 * write for the newest value reports success or failure.
 */
export interface LatestSaver<T> {
  set: (value: T) => void;
  /** Send the pending value now instead of waiting for the delay. */
  flush: () => void;
  /**
   * Stop for good: a value still waiting is sent now (after a write in
   * flight, if any), later `set` calls are ignored, and no write reports
   * success or failure any more.
   */
  dispose: () => void;
}

export interface LatestSaverOptions<T> {
  delayMs: number;
  onSaved: (value: T) => void;
  onError: (error: unknown) => void;
}

export function createLatestSaver<T>(save: (value: T) => Promise<unknown>, options: LatestSaverOptions<T>): LatestSaver<T> {
  let pending: { value: T } | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let inFlight = false;
  let disposed = false;

  function clearTimer() {
    if (timer != null) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function pump() {
    if (inFlight || !pending) return;
    const { value } = pending;
    pending = null;
    inFlight = true;
    save(value).then(
      () => {
        if (!pending && !disposed) options.onSaved(value);
      },
      (error: unknown) => {
        if (!pending && !disposed) options.onError(error);
      },
    ).finally(() => {
      inFlight = false;
      if (timer == null) pump();
    });
  }

  return {
    set(value) {
      if (disposed) return;
      pending = { value };
      clearTimer();
      timer = setTimeout(() => {
        timer = null;
        pump();
      }, options.delayMs);
    },
    flush() {
      clearTimer();
      pump();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      clearTimer();
      pump();
    },
  };
}
