/** One-line label for a memory row.
 * Turn titles are stored clipped. When the body keeps the rest of that same line, show it. */
export function memoryRowHeadline(memory: { title: string; body: string }): string {
  const title = memory.title.trim();
  const first =
    memory.body
      .split('\n')
      .map((line) => line.trim())
      .find((line) => line.length > 0) ?? '';
  if (title.length > 0 && first.startsWith(title) && first.length > title.length) {
    return first;
  }
  return memory.title;
}

/** Relative age through one day; older rows show a calendar date. */
export function memoryRowAge(updatedAtMs: number, nowMs: number): string {
  if (!Number.isFinite(updatedAtMs) || !Number.isFinite(nowMs)) return '';
  const delta = nowMs - updatedAtMs;
  if (delta < 45_000) return 'just now';
  if (delta < 60_000) return `${Math.floor(delta / 1000)}s ago`;
  const minutes = Math.floor(delta / 60_000);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours <= 24) return `${hours}h ago`;
  return new Date(updatedAtMs).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
}

/** Parse an ISO, SQLite UTC, unix-second, or unix-ms timestamp. */
export function instantMs(value: string | number | null | undefined): number | null {
  if (typeof value === 'number') {
    if (!Number.isFinite(value) || value <= 0) return null;
    return value > 1e12 ? value : value * 1000;
  }
  if (!value) return null;
  const trimmed = value.trim();
  if (/^\d+$/.test(trimmed)) return instantMs(Number(trimmed));
  const normalized = trimmed.replace(' ', 'T');
  const zoned = /(?:Z|[+-]\d{2}:?\d{2})$/i.test(normalized) ? normalized : `${normalized}Z`;
  const ms = Date.parse(zoned);
  return Number.isNaN(ms) ? null : ms;
}

function countCommits(body: string): number {
  const beforeOutcome = body.split('\n\nOutcome: ')[0];
  const start = beforeOutcome.indexOf('\n\nCommits:');
  if (start < 0) return 0;
  const section = beforeOutcome.slice(start + '\n\nCommits:'.length).split('\n\n')[0];
  return section.split('\n').filter((line) => /^- [0-9a-f]{7,40}\b/.test(line.trim())).length;
}

export function memoryRowCounts(memory: { files: string[]; body: string }): string {
  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;
  const commits = countCommits(memory.body);
  return [
    memory.files.length > 0 ? plural(memory.files.length, 'file') : '',
    commits > 0 ? plural(commits, 'commit') : '',
  ]
    .filter(Boolean)
    .join(' · ');
}
