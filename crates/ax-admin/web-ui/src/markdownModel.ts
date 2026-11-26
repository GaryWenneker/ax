// Tool results are compact Markdown or plain text with numbered source lines.
// This parser turns that text into a small block model. Rendering builds React
// elements from the model, so tool output can never inject markup.

export type Inline = { kind: 'text' | 'code' | 'strong'; text: string };

export type Block =
  | { type: 'heading'; level: number; inline: Inline[] }
  | { type: 'code'; language: string; text: string }
  | { type: 'table'; header: Inline[][]; rows: Inline[][][] }
  | { type: 'list'; ordered: boolean; items: Inline[][] }
  | { type: 'text'; inline: Inline[] };

export function parseInline(source: string): Inline[] {
  const parts: Inline[] = [];
  const pattern = /`([^`\n]+)`|\*\*([^*\n]+)\*\*/g;
  let last = 0;
  for (let match = pattern.exec(source); match; match = pattern.exec(source)) {
    if (match.index > last) parts.push({ kind: 'text', text: source.slice(last, match.index) });
    parts.push(match[1] !== undefined ? { kind: 'code', text: match[1] } : { kind: 'strong', text: match[2] });
    last = pattern.lastIndex;
  }
  if (last < source.length) parts.push({ kind: 'text', text: source.slice(last) });
  return parts;
}

const HEADING = /^(#{1,6})\s+(.*)$/;
const FENCE = /^\s*```\s*([\w+-]*)\s*$/;
const BULLET = /^\s*[-*]\s+(.*)$/;
const ORDERED = /^\s*\d+[.)]\s+(.*)$/;
// "  12 | let x" or "12: let x": numbered source from the graph.
const NUMBERED = /^\s*\d+\s*[|│:]\s?/;
const TABLE_RULE = /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?\s*$/;

function cells(line: string): Inline[][] {
  const trimmed = line.trim().replace(/^\|/, '').replace(/\|$/, '');
  return trimmed.split('|').map((cell) => parseInline(cell.trim()));
}

function isTableRow(line: string): boolean {
  return line.trim().startsWith('|') && line.trim().length > 1;
}

export function parseMarkdown(source: string): Block[] {
  const lines = source.replace(/\r\n?/g, '\n').split('\n');
  const blocks: Block[] = [];
  let text: string[] = [];
  const flush = () => {
    const body = text.join('\n').replace(/^\n+|\n+$/g, '');
    if (body) blocks.push({ type: 'text', inline: parseInline(body) });
    text = [];
  };
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    const fence = FENCE.exec(line);
    if (fence) {
      flush();
      const body: string[] = [];
      i += 1;
      while (i < lines.length && !FENCE.test(lines[i])) body.push(lines[i++]);
      i += 1; // closing fence, or the end of an unterminated block
      blocks.push({ type: 'code', language: fence[1], text: body.join('\n') });
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      flush();
      blocks.push({ type: 'heading', level: heading[1].length, inline: parseInline(heading[2].trim()) });
      i += 1;
      continue;
    }
    if (isTableRow(line) && i + 1 < lines.length && TABLE_RULE.test(lines[i + 1])) {
      flush();
      const header = cells(line);
      const rows: Inline[][][] = [];
      i += 2;
      while (i < lines.length && isTableRow(lines[i])) rows.push(cells(lines[i++]));
      blocks.push({ type: 'table', header, rows });
      continue;
    }
    if (NUMBERED.test(line) && i + 1 < lines.length && NUMBERED.test(lines[i + 1])) {
      flush();
      const body: string[] = [];
      while (i < lines.length && NUMBERED.test(lines[i])) body.push(lines[i++]);
      blocks.push({ type: 'code', language: 'source', text: body.join('\n') });
      continue;
    }
    const bullet = BULLET.exec(line);
    const ordered = bullet ? null : ORDERED.exec(line);
    if (bullet || ordered) {
      flush();
      const pattern = bullet ? BULLET : ORDERED;
      const items: Inline[][] = [];
      for (let item = pattern.exec(lines[i] ?? ''); item; item = pattern.exec(lines[i] ?? '')) {
        items.push(parseInline(item[1]));
        i += 1;
        if (i >= lines.length) break;
      }
      blocks.push({ type: 'list', ordered: !bullet, items });
      continue;
    }
    if (line.trim() === '') flush();
    else text.push(line);
    i += 1;
  }
  flush();
  return blocks;
}
