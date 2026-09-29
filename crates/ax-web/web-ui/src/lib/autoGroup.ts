export interface AutoGroupItem {
  key: string;
  text: string;
  group: string;
}

export interface AutoGroupDef {
  id: string;
  label: string;
  aliases?: string[];
}

export interface AutoGroupProposal {
  key: string;
  group: string | null;
  score: number;
}

export const AUTO_GROUP_THRESHOLD = 0.15;

const UNGROUPED = 'ungrouped';

const STOPWORDS = new Set(
  'the and for with that this from into then than are was were but not you your its has have had any all can use when each every only also must should will a an of to in on or is it be by as at'.split(' '),
);

type Vector = Map<string, number>;

export function tokens(text: string): string[] {
  return text
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((t) => t.length >= 3 && !STOPWORDS.has(t))
    .map((t) => (t.length > 4 && t.endsWith('s') && !t.endsWith('ss') ? t.slice(0, -1) : t));
}

function counts(words: string[]): Vector {
  const v: Vector = new Map();
  for (const w of words) v.set(w, (v.get(w) ?? 0) + 1);
  return v;
}

function weigh(tf: Vector, idf: Map<string, number>): Vector {
  const v: Vector = new Map();
  for (const [w, n] of tf) v.set(w, n * (idf.get(w) ?? 0));
  return v;
}

function cosine(a: Vector, b: Vector): number {
  let dot = 0;
  let na = 0;
  let nb = 0;
  for (const [w, x] of a) {
    na += x * x;
    const y = b.get(w);
    if (y) dot += x * y;
  }
  for (const y of b.values()) nb += y * y;
  return na && nb ? dot / Math.sqrt(na * nb) : 0;
}

/** For every ungrouped item, the existing group whose members (plus label and aliases) read most alike. */
export function proposeGroups(items: AutoGroupItem[], groups: AutoGroupDef[]): AutoGroupProposal[] {
  const targets = groups.filter((g) => g.id !== UNGROUPED);
  const docs = new Map<string, string[]>(
    targets.map((g) => [g.id, tokens([g.label, ...(g.aliases ?? [])].join(' '))]),
  );
  for (const item of items) docs.get(item.group)?.push(...tokens(item.text));
  const loose = items.filter((i) => i.group === UNGROUPED || !docs.has(i.group));
  const corpus = [...docs.values(), ...loose.map((i) => tokens(i.text))];
  const df = new Map<string, number>();
  for (const doc of corpus) for (const w of new Set(doc)) df.set(w, (df.get(w) ?? 0) + 1);
  const idf = new Map([...df].map(([w, n]) => [w, Math.log(1 + corpus.length / n)]));
  const groupVectors = targets.map((g) => ({ id: g.id, v: weigh(counts(docs.get(g.id)!), idf) }));

  return loose.map((item) => {
    const v = weigh(counts(tokens(item.text)), idf);
    let best: { id: string; score: number } | null = null;
    for (const g of groupVectors) {
      const score = cosine(v, g.v);
      if (!best || score > best.score) best = { id: g.id, score };
    }
    const score = best?.score ?? 0;
    return { key: item.key, group: best && score >= AUTO_GROUP_THRESHOLD ? best.id : null, score };
  });
}
