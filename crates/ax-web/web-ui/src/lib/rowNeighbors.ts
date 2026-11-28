/** Whether the row before / after `selected` exists in the rendered order `ids`. */
export interface NeighborNav {
  prev: boolean;
  next: boolean;
}

/** The id `delta` rows away from `selected` in `ids`, or null at an end or when `selected` is not shown. */
export function neighborId(ids: readonly string[], selected: string | null, delta: number): string | null {
  const current = selected ? ids.indexOf(selected) : -1;
  const next = current + delta;
  if (current < 0 || next < 0 || next >= ids.length) return null;
  return ids[next];
}

export function neighborNav(ids: readonly string[], selected: string | null): NeighborNav {
  return { prev: neighborId(ids, selected, -1) != null, next: neighborId(ids, selected, 1) != null };
}
