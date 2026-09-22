/** Matches `policy-blade-slide-in` / `policy-blade-slide-out` duration. */
export const POLICY_BLADE_DISMISS_MS = 320;

export function policyDetailOpen(selected: string | null | undefined, closing: boolean): boolean {
  return Boolean(selected) || closing;
}

export function policyWorkspaceHostClass(closing: boolean): string {
  return closing ? 'policy-inline-host policy-inline-host--closing' : 'policy-inline-host';
}
