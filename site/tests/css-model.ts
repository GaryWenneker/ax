// A small CSS model for tests: parses a stylesheet into rules, media blocks and
// keyframes, and normalizes values so assertions check what the CSS means, not
// how it is formatted. `blur(.4px)` and `blur( 0.4px )` compare equal, so do
// `#00FFFF` and `#00ffff`, and a reformatted file gives the same model.

export type Decls = Record<string, string>;
export interface Rule { selectors: string[]; media: string; decls: Decls }
export interface Sheet { rules: Rule[]; keyframes: Record<string, Record<string, Decls>> }

export function normalizeValue(value: string): string {
  return value
    .trim()
    .replace(/"/g, "'")
    .replace(/\s+/g, ' ')
    .replace(/\s*([,()])\s*/g, '$1')
    .replace(/\s*!\s*important$/i, '!important')
    .replace(/(^|[^\w.#])(-?)\.(\d)/g, '$1$20.$3')
    .replace(/#[0-9a-f]{3,8}\b/gi, (hex) => hex.toLowerCase());
}

export function normalizeSelector(selector: string): string {
  return selector.trim().replace(/"/g, "'").replace(/\s+/g, ' ').replace(/\s*([>+~,])\s*/g, '$1');
}

export function normalizeMedia(prelude: string): string {
  return prelude.trim().replace(/^@media\b/i, '').replace(/\s+/g, ' ').replace(/\s*([():,])\s*/g, '$1').trim();
}

/** Reads from `i` to the first top-level character in `stops`, skipping quotes and parentheses. */
function readUntil(src: string, i: number, stops: string): { text: string; end: number } {
  let depth = 0;
  let quote = '';
  let j = i;
  for (; j < src.length; j++) {
    const c = src[j];
    if (quote) {
      if (c === '\\') j++;
      else if (c === quote) quote = '';
    } else if (c === '"' || c === "'") quote = c;
    else if (c === '(') depth++;
    else if (c === ')') depth--;
    else if (depth === 0 && stops.includes(c)) break;
  }
  return { text: src.slice(i, j), end: j };
}

function readDecls(src: string, i: number): { decls: Decls; end: number } {
  const decls: Decls = {};
  for (;;) {
    const { text, end } = readUntil(src, i, ';}');
    const colon = text.indexOf(':');
    if (colon > 0) decls[text.slice(0, colon).trim().toLowerCase()] = normalizeValue(text.slice(colon + 1));
    if (end >= src.length || src[end] === '}') return { decls, end: end + 1 };
    i = end + 1;
  }
}

function parseBlock(src: string, i: number, media: string, sheet: Sheet): number {
  while (i < src.length) {
    while (/\s/.test(src[i] ?? '')) i++;
    if (i >= src.length) return i;
    if (src[i] === '}') return i + 1;
    const { text, end } = readUntil(src, i, '{;}');
    if (src[end] !== '{') {
      i = end + 1;
      continue;
    }
    const prelude = text.trim();
    const frames = prelude.match(/^@(?:-webkit-)?keyframes\s+(\S+)/i);
    if (/^@(media|supports|layer|container)\b/i.test(prelude)) {
      const inner = /^@media\b/i.test(prelude) ? normalizeMedia(prelude) : normalizeSelector(prelude);
      i = parseBlock(src, end + 1, media ? `${media} and ${inner}` : inner, sheet);
    } else if (frames) {
      const stops: Record<string, Decls> = {};
      i = end + 1;
      for (;;) {
        while (/\s/.test(src[i] ?? '')) i++;
        if (i >= src.length || src[i] === '}') { i++; break; }
        const stop = readUntil(src, i, '{}');
        const { decls, end: after } = readDecls(src, stop.end + 1);
        for (const key of stop.text.split(',')) {
          const k = key.trim().toLowerCase().replace(/^from$/, '0%').replace(/^to$/, '100%');
          stops[k] = { ...stops[k], ...decls };
        }
        i = after;
      }
      sheet.keyframes[frames[1]] = stops;
    } else {
      const { decls, end: after } = readDecls(src, end + 1);
      sheet.rules.push({ selectors: normalizeSelector(prelude).split(','), media, decls });
      i = after;
    }
  }
  return i;
}

export function parseCss(source: string): Sheet {
  const sheet: Sheet = { rules: [], keyframes: {} };
  parseBlock(source.replace(/\/\*[\s\S]*?\*\//g, ''), 0, '', sheet);
  return sheet;
}

/** Declarations of every rule (in source order) that lists `selector` under `media`. */
export function rulesFor(sheet: Sheet, selector: string, media = ''): Decls[] {
  const want = normalizeSelector(selector);
  const at = normalizeMedia(media);
  return sheet.rules.filter((r) => r.media === at && r.selectors.includes(want)).map((r) => r.decls);
}

/** The value a later rule leaves for `prop` on `selector` under `media`, normalized. */
export function decl(sheet: Sheet, selector: string, prop: string, media = ''): string | undefined {
  let value: string | undefined;
  for (const decls of rulesFor(sheet, selector, media)) if (prop in decls) value = decls[prop];
  return value;
}

/** Every declaration in the sheet, for whole-sheet checks such as "no external images". */
export function allDecls(sheet: Sheet): { selector: string; media: string; prop: string; value: string }[] {
  return sheet.rules.flatMap((r) =>
    Object.entries(r.decls).map(([prop, value]) => ({ selector: r.selectors.join(','), media: r.media, prop, value })),
  );
}

/** Pretty-prints a stylesheet the way a formatter would: one declaration per line, spaces after colons. */
export function reformat(source: string): string {
  return source
    .replace(/\{/g, ' {\n  ')
    .replace(/;(?![^(]*\))/g, ';\n  ')
    .replace(/\}/g, '\n}\n')
    .replace(/:(?=[^\s/])(?![^{}]*\{)/g, ': ')
    .replace(/(^|[\s,(:])\.(\d)/g, '$10.$2')
    .replace(/#(?:00ffff|ff00ff|ffff00)\b/g, (hex) => hex.toUpperCase());
}
