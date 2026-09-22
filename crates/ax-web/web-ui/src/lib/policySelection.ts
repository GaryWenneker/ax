export type SelectionGesture = {
  id: string;
  visibleIds: string[];
  selected: Set<string>;
  anchor: string | null;
  metaKey: boolean;
  shiftKey: boolean;
};

export type SelectionResult = {
  selected: Set<string>;
  anchor: string | null;
  /** Inline editor target; null when multi-select or none. */
  openId: string | null;
};

export function nextRowSelection(g: SelectionGesture): SelectionResult {
  const { id, visibleIds, metaKey, shiftKey } = g;
  if (shiftKey && g.anchor && visibleIds.includes(g.anchor) && visibleIds.includes(id)) {
    const a = visibleIds.indexOf(g.anchor);
    const b = visibleIds.indexOf(id);
    const [lo, hi] = a < b ? [a, b] : [b, a];
    const selected = new Set(visibleIds.slice(lo, hi + 1));
    const openId = selected.size === 1 ? id : null;
    return { selected, anchor: g.anchor, openId };
  }
  if (metaKey) {
    const selected = new Set(g.selected);
    if (selected.has(id)) selected.delete(id);
    else selected.add(id);
    const only = selected.size === 1 ? [...selected][0] : null;
    return { selected, anchor: id, openId: only };
  }
  return { selected: new Set([id]), anchor: id, openId: id };
}

export function toggleVisibleSelection(visibleIds: string[], selected: Set<string>): Set<string> {
  if (visibleIds.length === 0) return new Set();
  const allOn = visibleIds.every((id) => selected.has(id));
  return allOn ? new Set() : new Set(visibleIds);
}

export function menuTargets<T extends { id?: string; name?: string }>(
  clicked: T,
  selected: Set<string>,
  rows: T[],
  keyOf: (row: T) => string,
): T[] {
  const key = keyOf(clicked);
  if (selected.has(key) && selected.size > 1) {
    return rows.filter((r) => selected.has(keyOf(r)));
  }
  return [clicked];
}
