/** Service worker opt-in: the `pwa` query parameter, the stored choice, and standalone mode. */

export const PWA_OPTIN_KEY = 'ax-pwa-optin';

export type PwaOptIn = { enable: boolean; clearStored: boolean };

/**
 * Whether to register the service worker. `?pwa=1` opts in for this load;
 * `?pwa=0` (or `false`/`off`/`no`) opts out and clears the stored opt-in.
 * Without a parameter, a stored opt-in or an installed standalone window
 * keeps the worker. Any other value is ignored.
 */
export function resolvePwaOptIn(param: string | null, stored: boolean, standalone: boolean): PwaOptIn {
  const value = param?.trim().toLowerCase() ?? null;
  if (value === '1') return { enable: true, clearStored: false };
  if (value === '0' || value === 'false' || value === 'off' || value === 'no') {
    return { enable: false, clearStored: true };
  }
  return { enable: stored || standalone, clearStored: false };
}
