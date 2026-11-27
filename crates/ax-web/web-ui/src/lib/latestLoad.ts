/** Helpers for pages that reload the same data on demand and on live events. */

/** Numbers each request so a slow, older response cannot overwrite a newer one. */
export function createLatestGate(): { begin: () => number; isLatest: (ticket: number) => boolean } {
  let latest = 0;
  return {
    begin: () => {
      latest += 1;
      return latest;
    },
    isLatest: (ticket) => ticket === latest,
  };
}

/**
 * The data to show after a load fails. A quiet (background) refresh keeps what
 * is on screen; a load the user asked for (a new period) clears it so old
 * numbers are not shown under the new choice.
 */
export function dataAfterFailedLoad<T>(shown: T | null, quiet: boolean): T | null {
  return quiet ? shown : null;
}
