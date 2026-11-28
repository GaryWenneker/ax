/** True while `text` has a `{` that is not closed yet, as in the glob `*.{ts,`. */
export function insideBraces(text: string): boolean {
  let depth = 0;
  for (const ch of text) {
    if (ch === '{') depth++;
    else if (ch === '}' && depth > 0) depth--;
  }
  return depth > 0;
}

/**
 * Split a comma-separated list of globs, triggers or tags. A comma inside `{...}` belongs to the
 * value, so the brace glob `src/**\/*.{ts,tsx}` stays one entry. Entries are trimmed and empty
 * ones dropped.
 */
export function splitPolicyList(text: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let current = '';
  for (const ch of text) {
    if (ch === '{') depth++;
    else if (ch === '}' && depth > 0) depth--;
    if (ch === ',' && depth === 0) {
      out.push(current);
      current = '';
    } else {
      current += ch;
    }
  }
  out.push(current);
  return cleanPolicyList(out);
}

/** Trim each entry and drop empty ones. Entries are never split. */
export function cleanPolicyList(values: readonly string[]): string[] {
  return values.map((x) => x.trim()).filter(Boolean);
}

/**
 * Add the entries of `text` (split like `splitPolicyList`, so a pasted `react, frontend`
 * gives two entries and a brace glob stays whole) to `values`, skipping any entry that is
 * already there, compared without regard to case.
 */
export function addPolicyEntries(values: readonly string[], text: string): string[] {
  const next = [...values];
  const seen = new Set(values.map((item) => item.toLowerCase()));
  for (const entry of splitPolicyList(text)) {
    const key = entry.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    next.push(entry);
  }
  return next;
}

/**
 * The draft after pasting `text` over the selection `start..end`, as the browser would put it.
 * A missing caret means the end of the draft; a missing `end` means no selection. Positions
 * are clamped to the draft and a reversed range is put in order.
 */
export function pasteIntoDraft(
  draft: string,
  text: string,
  start: number | null,
  end: number | null,
): string {
  const clamp = (n: number) => Math.min(Math.max(n, 0), draft.length);
  const a = clamp(start ?? draft.length);
  const b = clamp(end ?? a);
  const [from, to] = a <= b ? [a, b] : [b, a];
  return draft.slice(0, from) + text + draft.slice(to);
}
