/**
 * Folds any number of requests into one call on the next animation frame, so a
 * burst of DOM changes costs one run instead of one per change.
 */
export function coalesceToFrame(
  run: () => void,
  schedule: (cb: () => void) => number = (cb) => requestAnimationFrame(cb),
  cancelFrame: (id: number) => void = (id) => cancelAnimationFrame(id),
): { request: () => void; cancel: () => void } {
  let pending: number | null = null;
  return {
    request: () => {
      if (pending !== null) return;
      pending = schedule(() => {
        pending = null;
        run();
      });
    },
    cancel: () => {
      if (pending === null) return;
      cancelFrame(pending);
      pending = null;
    },
  };
}
