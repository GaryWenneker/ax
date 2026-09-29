export interface ConversationLink {
  key: string;
  hue: number;
  position: number;
  count: number;
  joinPrev: boolean;
  joinNext: boolean;
}

const KEY_LINE = /^Conversation: ([0-9a-f]{12})$/m;

export function conversationKey(body: string): string | null {
  return KEY_LINE.exec(body)?.[1] ?? null;
}

/** Rows are newest first; conversations with one shown turn get `null`. */
export function conversationLinks(rows: { body: string }[]): (ConversationLink | null)[] {
  const keys = rows.map((r) => conversationKey(r.body));
  const counts = new Map<string, number>();
  for (const k of keys) if (k) counts.set(k, (counts.get(k) ?? 0) + 1);
  const seen = new Map<string, number>();
  return keys.map((key, i) => {
    const count = key ? counts.get(key) ?? 0 : 0;
    if (!key || count < 2) return null;
    const n = seen.get(key) ?? 0;
    seen.set(key, n + 1);
    return {
      key,
      hue: parseInt(key.slice(0, 8), 16) % 360,
      position: count - n,
      count,
      joinPrev: keys[i - 1] === key,
      joinNext: keys[i + 1] === key,
    };
  });
}
