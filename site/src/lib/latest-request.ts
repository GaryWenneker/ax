/** One in-flight request handle; only the newest handle is current. */
export interface LatestRequest {
  signal: AbortSignal;
  /** False once a newer request started: its response must be ignored. */
  isCurrent(): boolean;
  /** Aborts this request (for a timeout); it stays current so its error is shown. */
  abort(): void;
}

/**
 * Tracks overlapping loads where only the last one may update the page.
 * begin() aborts the previous request and marks it stale.
 */
export function latestRequests(): { begin(): LatestRequest } {
  let generation = 0;
  let current: AbortController | null = null;
  return {
    begin() {
      current?.abort();
      const controller = new AbortController();
      const id = ++generation;
      current = controller;
      return {
        signal: controller.signal,
        isCurrent: () => id === generation,
        abort: () => controller.abort(),
      };
    },
  };
}
