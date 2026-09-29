export type LinkKind = 'rule' | 'skill' | 'memory';

export interface LinkTarget {
  key: string;
  kind: LinkKind;
  label: string;
  origin: string;
  target: string;
  tags?: string[];
}

export const LINK_PICKER_MAX = 100;

const KINDS: readonly string[] = ['rule', 'skill', 'memory'] satisfies LinkKind[];

export function isLinkTarget(v: unknown): v is LinkTarget {
  if (typeof v !== 'object' || v === null) return false;
  const o = v as Record<string, unknown>; // a non-null object; every field is checked below
  return (
    typeof o.key === 'string' &&
    typeof o.kind === 'string' &&
    KINDS.includes(o.kind) &&
    typeof o.label === 'string' &&
    typeof o.origin === 'string' &&
    typeof o.target === 'string' &&
    o.target !== '' &&
    (o.tags === undefined || (Array.isArray(o.tags) && o.tags.every((t) => typeof t === 'string')))
  );
}

export function linkText(item: LinkTarget): string {
  return `[[${item.target}]]`;
}

/** The `[[` query being typed just before `caret`, or null when the caret is not inside one. */
export function openLinkQuery(text: string, caret: number): { start: number; query: string } | null {
  const before = text.slice(0, caret);
  const start = before.lastIndexOf('[[');
  if (start < 0) return null;
  const query = before.slice(start + 2);
  if (/[\]\n[]/.test(query)) return null;
  return { start, query };
}

export interface LinkQuery {
  kind: LinkKind | null;
  words: string[];
  tags: string[];
}

const KIND_PREFIX: Partial<Record<string, LinkKind>> = {
  rule: 'rule', r: 'rule', skill: 'skill', s: 'skill', memory: 'memory', m: 'memory',
};

/** `s:#azure review` → kind skill, tag `azure`, word `review`. */
export function parseLinkQuery(query: string): LinkQuery {
  let rest = query.trim().toLowerCase();
  let kind: LinkKind | null = null;
  const prefix = /^([a-z]+):/.exec(rest);
  const prefixKind = prefix ? KIND_PREFIX[prefix[1]] : undefined;
  if (prefix && prefixKind) {
    kind = prefixKind;
    rest = rest.slice(prefix[0].length);
  }
  const words: string[] = [];
  const tags: string[] = [];
  for (const term of rest.split(/\s+/)) {
    if (term.startsWith('#')) {
      if (term.length > 1) tags.push(term.slice(1));
    } else if (term) {
      words.push(term);
    }
  }
  return { kind, words, tags };
}

export function isUrlQuery(query: string): boolean {
  return /^https?:\/\/\S+$/i.test(query.trim());
}

const KIND_ORDER: Record<LinkKind, number> = { rule: 0, skill: 1, memory: 2 };

function nameRank(item: LinkTarget, words: string[]): number {
  if (words.length === 0) return 0;
  const names = [item.label.toLowerCase(), item.target.toLowerCase()];
  const first = words[0];
  if (names.some((n) => n === first)) return 0;
  if (names.some((n) => n.startsWith(first))) return 1;
  if (names.some((n) => n.includes(first))) return 2;
  return 3;
}

function matches(item: LinkTarget, q: LinkQuery): boolean {
  const tags = (item.tags ?? []).map((t) => t.toLowerCase());
  const names = [item.label.toLowerCase(), item.target.toLowerCase()];
  return (
    q.tags.every((t) => tags.some((x) => x.startsWith(t))) &&
    q.words.every((w) => names.some((n) => n.includes(w)) || tags.some((x) => x.includes(w)))
  );
}

/** Up to `max` rows, shared across kinds so a long first kind cannot hide the others. */
function capAcrossKinds(sorted: LinkTarget[], max: number): LinkTarget[] {
  if (sorted.length <= max) return sorted;
  const counts = new Map<LinkKind, number>();
  for (const i of sorted) counts.set(i.kind, (counts.get(i.kind) ?? 0) + 1);
  const quota = new Map<LinkKind, number>();
  let left = max;
  let open = [...counts.keys()];
  while (left > 0 && open.length > 0) {
    const share = Math.max(1, Math.floor(left / open.length));
    for (const k of open) {
      const room = (counts.get(k) ?? 0) - (quota.get(k) ?? 0);
      const add = Math.min(share, room, left);
      quota.set(k, (quota.get(k) ?? 0) + add);
      left -= add;
    }
    open = open.filter((k) => (quota.get(k) ?? 0) < (counts.get(k) ?? 0));
  }
  const used = new Map<LinkKind, number>();
  return sorted.filter((i) => {
    const n = used.get(i.kind) ?? 0;
    if (n >= (quota.get(i.kind) ?? 0)) return false;
    used.set(i.kind, n + 1);
    return true;
  });
}

export function searchLinkTargets(
  items: LinkTarget[],
  query: string,
  opts: { kind?: LinkKind | null; selfKey?: string } = {},
): { items: LinkTarget[]; more: number; counts: Record<LinkKind, number> } {
  const q = parseLinkQuery(query);
  const kind = q.kind ?? opts.kind ?? null;
  const hits = items
    .map((item, index) => ({ item, index }))
    .filter(({ item }) => item.key !== opts.selfKey && (!kind || item.kind === kind) && matches(item, q))
    .map((h) => ({ ...h, rank: nameRank(h.item, q.words) }))
    .sort((a, b) => a.rank - b.rank || KIND_ORDER[a.item.kind] - KIND_ORDER[b.item.kind] || a.index - b.index)
    .map((h) => h.item);
  const shown = capAcrossKinds(hits, LINK_PICKER_MAX);
  const counts: Record<LinkKind, number> = { rule: 0, skill: 0, memory: 0 };
  for (const h of hits) counts[h.kind] += 1;
  return { items: shown, more: hits.length - shown.length, counts };
}

/** The inside of `[[…]]`: the target, plus `|label` when text was selected. */
export function wikiRaw(item: LinkTarget, label: string): string {
  const clean = label.replace(/\]\]|[|\n\]]/g, ' ').replace(/\s+/g, ' ').trim();
  return clean ? `${item.target}|${clean}` : item.target;
}
