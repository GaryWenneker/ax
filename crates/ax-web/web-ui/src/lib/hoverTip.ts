/** Elements that already take keyboard focus; a HoverTip around one needs no tab stop of its own. */
export const FOCUSABLE_SELECTOR =
  'button, a[href], input, select, textarea, summary, [contenteditable="true"], [tabindex]:not([tabindex="-1"])';

/** Adds `id` to an aria-describedby value once. */
export function withDescribedBy(existing: string | null, id: string): string {
  const tokens = (existing ?? '').split(/\s+/).filter(Boolean);
  if (!tokens.includes(id)) tokens.push(id);
  return tokens.join(' ');
}

/** Removes `id` from an aria-describedby value; null when nothing is left. */
export function withoutDescribedBy(existing: string | null, id: string): string | null {
  const tokens = (existing ?? '').split(/\s+/).filter((t) => t && t !== id);
  return tokens.length ? tokens.join(' ') : null;
}
