/**
 * Provenance checks for screenshots referenced from the docs.
 * `entry` is a row from site/public/screenshots/manifest.json (or undefined),
 * `hash` is the sha256 of the image on disk. Returns the issues found.
 */
export function screenshotEntryIssues(name, entry, hash) {
  if (!entry) return [`screenshot provenance missing: ${name}`];
  const issues = [];
  if (entry.imageHash !== hash) issues.push(`screenshot hash missing/stale: ${name}`);
  const hasSource = entry.state || String(entry.data ?? '').startsWith('public');
  if (!entry.sourceHash || !hasSource) issues.push(`screenshot source/state missing: ${name}`);
  return issues;
}
