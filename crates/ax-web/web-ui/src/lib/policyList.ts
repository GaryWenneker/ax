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
