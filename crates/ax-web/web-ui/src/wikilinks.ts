/** Obsidian-style `[[links]]` in rule, skill and memory bodies. */

export type LinkKind = 'rule' | 'skill' | 'memory';

export interface LinkRef {
  kind: LinkKind;
  id: string;
  origin: 'project' | 'global';
  projectId?: number;
}

export interface OutgoingLink {
  text: string;
  target: string;
  heading: string | null;
  label: string | null;
  resolved: LinkRef | null;
}

export interface Backlink extends LinkRef {
  title: string;
}

export interface ItemLinks {
  outgoing: OutgoingLink[];
  backlinks: Backlink[];
}

export type Segment = { text: string } | { link: string };

export const MISSING_HREF = '#ax-missing-link';

export async function fetchItemLinks(kind: LinkKind, id: string, origin?: string): Promise<ItemLinks> {
  const params = new URLSearchParams({ kind, id });
  if (origin === 'global') params.set('origin', 'global');
  const res = await fetch(`/api/links?${params}`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json() as Promise<ItemLinks>;
}

/** A fence line: ``` or ~~~, three or more, after leading whitespace. */
function fenceMarker(line: string): [string, number] | null {
  const t = line.trimStart();
  const c = t[0];
  if (c !== '`' && c !== '~') return null;
  let n = 0;
  while (t[n] === c) n += 1;
  return n >= 3 ? [c, n] : null;
}

/** Same rules as `ax_policy::links::wiki_link`: no nested `[[`, a non-empty target. */
function isLink(inner: string): boolean {
  if (inner.includes('[[')) return false;
  const dest = inner.split('|')[0];
  return dest.split('#')[0].trim() !== '';
}

function splitLine(line: string, out: Segment[]): void {
  let text = '';
  let i = 0;
  while (i < line.length) {
    if (line[i] === '`') {
      let run = 0;
      while (line[i + run] === '`') run += 1;
      const ticks = line.slice(i, i + run);
      const end = line.indexOf(ticks, i + run);
      const next = end === -1 ? i + run : end + run;
      text += line.slice(i, next);
      i = next;
      continue;
    }
    if (line.startsWith('[[', i)) {
      const end = line.indexOf(']]', i + 2);
      if (end !== -1) {
        const whole = line.slice(i, end + 2);
        if (isLink(line.slice(i + 2, end))) {
          const embed = text.endsWith('!');
          if (embed) text = text.slice(0, -1);
          if (text) out.push({ text });
          out.push({ link: embed ? `!${whole}` : whole });
          text = '';
        } else {
          text += whole;
        }
        i = end + 2;
        continue;
      }
    }
    text += line[i];
    i += 1;
  }
  if (text) out.push({ text });
}

/** The body as text and `[[link]]` segments, skipping fenced and inline code. */
export function splitWikiLinks(body: string): Segment[] {
  const raw: Segment[] = [];
  let fence: [string, number] | null = null;
  for (const line of body.split(/(?<=\n)/)) {
    const marker = fenceMarker(line);
    if (marker) {
      if (!fence) fence = marker;
      else if (marker[0] === fence[0] && marker[1] >= fence[1]) fence = null;
      raw.push({ text: line });
    } else if (fence) {
      raw.push({ text: line });
    } else {
      splitLine(line, raw);
    }
  }
  const out: Segment[] = [];
  for (const seg of raw) {
    const last = out[out.length - 1];
    if ('text' in seg && last && 'text' in last) last.text += seg.text;
    else out.push(seg);
  }
  return out;
}

/** Command Center URL of a linked item. */
export function linkHref(ref: LinkRef): string {
  const params = new URLSearchParams();
  let path: string;
  if (ref.kind === 'rule') {
    path = '/policy/rules';
    params.set('id', ref.id);
  } else if (ref.kind === 'skill') {
    path = '/policy/skills';
    params.set('name', ref.id);
  } else {
    path = '/memory';
    params.set('id', ref.id);
  }
  if (ref.origin === 'global') {
    params.set('origin', 'global');
    if (ref.projectId != null) params.set('projectId', String(ref.projectId));
  }
  return `${path}?${params}`;
}

export function missingTitle(target: string): string {
  return `No rule, skill or memory named ${target}`;
}

function escapeLinkText(s: string): string {
  return s.replace(/[\\[\]]/g, (c) => `\\${c}`);
}

function escapeTitle(s: string): string {
  return s.replace(/[\\"]/g, (c) => `\\${c}`);
}

/** Markdown for the body with each known `[[link]]` as a link. Code and links the server
 *  did not report stay exactly as written. */
export function wikiLinksToMarkdown(body: string, outgoing: OutgoingLink[]): string {
  const byText = new Map(outgoing.map((l) => [l.text, l]));
  return splitWikiLinks(body)
    .map((seg) => {
      if ('text' in seg) return seg.text;
      const link = byText.get(seg.link);
      if (!link) return seg.link;
      const text = escapeLinkText(link.label ?? link.target);
      if (!link.resolved) return `[${text}](${MISSING_HREF} "${escapeTitle(missingTitle(link.target))}")`;
      return `[${text}](${linkHref(link.resolved)})`;
    })
    .join('');
}
