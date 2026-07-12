export const WORKSPACE_SWITCHED = 'ax-workspace-switched';

/** Broadcast after the server has switched the active ax project. */
export function notifyWorkspaceSwitched(path?: string) {
  window.dispatchEvent(new CustomEvent(WORKSPACE_SWITCHED, { detail: { path } }));
}
