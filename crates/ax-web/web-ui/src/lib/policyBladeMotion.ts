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

/**
 * Clicks that must not dismiss the open skill/rule blade.
 * The properties dialog and its select menus are portaled to document.body,
 * outside the blade host, so a pointerdown there is not `host.contains(target)`.
 */
export const POLICY_BLADE_POINTER_KEEP =
  '.ax-modal, .ax-modal-overlay, .settings-select-menu, .policy-context-menu, .policy-calm-row, .policy-list-resize-handle';

type PointerHit = {
  closest?: (selector: string) => unknown;
  parentElement?: PointerHit | null;
};

export function pointerKeepsPolicyBlade(
  target: PointerHit | null,
  host: { contains(node: never): boolean } | null,
): boolean {
  if (!target) return false;
  if (host?.contains(target as never)) return true;
  const el = typeof target.closest === 'function' ? target : target.parentElement ?? null;
  return Boolean(el?.closest?.(POLICY_BLADE_POINTER_KEEP));
}
