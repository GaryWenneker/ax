export type DiffLine = { kind: 'context' | 'add' | 'remove'; text: string };

export type DiffHunk = {
  oldStart: number;
  oldCount: number;
  newStart: number;
  newCount: number;
  lines: DiffLine[];
};

const CONTEXT = 3;

function linesOf(text: string): string[] {
  if (text.length === 0) return [];
  const lines = text.split('\n');
  if (text.endsWith('\n')) lines.pop();
  return lines;
}

/** Above this many LCS cells the changed middle is shown as one remove-then-add block. */
const MAX_TABLE_CELLS = 4_000_000;

function lcsScript(before: string[], after: string[], script: DiffLine[]): void {
  const rows = before.length;
  const cols = after.length;
  if (rows === 0 || cols === 0 || (rows + 1) * (cols + 1) > MAX_TABLE_CELLS) {
    for (const text of before) script.push({ kind: 'remove', text });
    for (const text of after) script.push({ kind: 'add', text });
    return;
  }
  const width = cols + 1;
  const score = new Uint32Array((rows + 1) * width);
  for (let row = rows - 1; row >= 0; row -= 1) {
    for (let col = cols - 1; col >= 0; col -= 1) {
      score[row * width + col] = before[row] === after[col]
        ? score[(row + 1) * width + col + 1] + 1
        : Math.max(score[(row + 1) * width + col], score[row * width + col + 1]);
    }
  }
  let row = 0;
  let col = 0;
  while (row < rows && col < cols) {
    if (before[row] === after[col]) {
      script.push({ kind: 'context', text: before[row] });
      row += 1;
      col += 1;
    } else if (score[(row + 1) * width + col] >= score[row * width + col + 1]) {
      script.push({ kind: 'remove', text: before[row] });
      row += 1;
    } else {
      script.push({ kind: 'add', text: after[col] });
      col += 1;
    }
  }
  while (row < rows) {
    script.push({ kind: 'remove', text: before[row] });
    row += 1;
  }
  while (col < cols) {
    script.push({ kind: 'add', text: after[col] });
    col += 1;
  }
}

function editScript(before: string[], after: string[]): DiffLine[] {
  // Most revisions change a few lines: diff only the part between the shared head and tail.
  let head = 0;
  while (head < before.length && head < after.length && before[head] === after[head]) head += 1;
  let tail = 0;
  while (
    tail < before.length - head &&
    tail < after.length - head &&
    before[before.length - 1 - tail] === after[after.length - 1 - tail]
  ) tail += 1;
  const script: DiffLine[] = [];
  for (let index = 0; index < head; index += 1) script.push({ kind: 'context', text: before[index] });
  lcsScript(before.slice(head, before.length - tail), after.slice(head, after.length - tail), script);
  for (let index = before.length - tail; index < before.length; index += 1) script.push({ kind: 'context', text: before[index] });
  return script;
}

function hunkRanges(script: DiffLine[]): Array<[number, number]> {
  const ranges: Array<[number, number]> = [];
  script.forEach((line, index) => {
    if (line.kind === 'context') return;
    const start = Math.max(0, index - CONTEXT);
    const end = Math.min(script.length - 1, index + CONTEXT);
    const last = ranges[ranges.length - 1];
    if (last && start <= last[1] + 1) last[1] = Math.max(last[1], end);
    else ranges.push([start, end]);
  });
  return ranges;
}

export function unifiedDiff(before: string, after: string): DiffHunk[] {
  const script = editScript(linesOf(before), linesOf(after));
  // One running pass over the script gives every hunk's start lines.
  let scanned = 0;
  let oldLine = 1;
  let newLine = 1;
  return hunkRanges(script).map(([start, end]) => {
    for (; scanned < start; scanned += 1) {
      if (script[scanned].kind !== 'add') oldLine += 1;
      if (script[scanned].kind !== 'remove') newLine += 1;
    }
    const lines = script.slice(start, end + 1);
    return {
      oldStart: oldLine,
      oldCount: lines.filter((line) => line.kind !== 'add').length,
      newStart: newLine,
      newCount: lines.filter((line) => line.kind !== 'remove').length,
      lines,
    };
  });
}

export function formatUnifiedDiff(hunks: DiffHunk[]): string {
  return hunks.map((hunk) => {
    const header = `@@ -${hunk.oldStart},${hunk.oldCount} +${hunk.newStart},${hunk.newCount} @@`;
    const body = hunk.lines.map((line) => {
      const prefix = line.kind === 'add' ? '+' : line.kind === 'remove' ? '-' : ' ';
      return `${prefix}${line.text}`;
    });
    return [header, ...body].join('\n');
  }).join('\n');
}
