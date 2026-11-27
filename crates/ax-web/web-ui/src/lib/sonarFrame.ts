/** What the SonarQube dashboard panel shows: a spinner, the frame, or the offline panel. */
export type SonarFrameState = 'loading' | 'loaded' | 'error';

/**
 * The panel state after the dashboard frame fires `load` or `error`.
 * A frame also loads when the proxy answers with an error page, so `load`
 * only reveals the frame while it is still loading and never hides an error
 * the status probe already found.
 */
export function frameStateAfter(current: SonarFrameState, event: 'load' | 'error'): SonarFrameState {
  if (event === 'error') return 'error';
  return current === 'loading' ? 'loaded' : current;
}
