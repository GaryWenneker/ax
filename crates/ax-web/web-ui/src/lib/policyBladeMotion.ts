/** Matches `policy-blade-slide-in` / `policy-blade-slide-out` duration. */
export const POLICY_BLADE_DISMISS_MS = 320;

export function policyDetailOpen(selected: string | null | undefined, closing: boolean): boolean {
  return Boolean(selected) || closing;
}

/** A selection from the URL may name an item the list has not loaded yet; only a loaded list can hide it. */
export function selectionHidden(selected: string | null, visibleIds: readonly string[], loading: boolean): boolean {
  return Boolean(selected) && !loading && !visibleIds.includes(selected as string);
}

export function policyWorkspaceHostClass(closing: boolean): string {
  return closing ? 'policy-inline-host policy-inline-host--closing' : 'policy-inline-host';
}
