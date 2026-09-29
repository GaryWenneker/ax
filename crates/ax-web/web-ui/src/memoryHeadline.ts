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

/** Compact age for a memory row, like a commit graph ("18h ago"). */
export function memoryRowAge(updatedAtMs: number, nowMs: number): string {
  if (!Number.isFinite(updatedAtMs) || !Number.isFinite(nowMs)) return '';
  const delta = nowMs - updatedAtMs;
  if (delta < 45_000) return 'just now';
  const minutes = Math.floor(delta / 60_000);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
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
