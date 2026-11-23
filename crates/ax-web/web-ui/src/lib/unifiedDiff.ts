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

function editScript(before: string[], after: string[]): DiffLine[] {
  const rows = before.length;
  const cols = after.length;
  const score: number[][] = Array.from({ length: rows + 1 }, () => Array(cols + 1).fill(0));
  for (let row = rows - 1; row >= 0; row -= 1) {
    for (let col = cols - 1; col >= 0; col -= 1) {
      score[row][col] = before[row] === after[col]
        ? score[row + 1][col + 1] + 1
        : Math.max(score[row + 1][col], score[row][col + 1]);
    }
  }
  const script: DiffLine[] = [];
  let row = 0;
  let col = 0;
  while (row < rows && col < cols) {
    if (before[row] === after[col]) {
      script.push({ kind: 'context', text: before[row] });
      row += 1;
      col += 1;
    } else if (score[row + 1][col] >= score[row][col + 1]) {
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
  return hunkRanges(script).map(([start, end]) => {
    let oldStart = 1;
    let newStart = 1;
    for (let index = 0; index < start; index += 1) {
      if (script[index].kind !== 'add') oldStart += 1;
      if (script[index].kind !== 'remove') newStart += 1;
    }
    const lines = script.slice(start, end + 1);
    return {
      oldStart,
      oldCount: lines.filter((line) => line.kind !== 'add').length,
      newStart,
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
