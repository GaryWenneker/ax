/** Identifies one item's links; empty when there is no item id yet. */
export function itemLinksKey(kind: string, id: string | null | undefined, origin: string | undefined): string {
  return id ? `${kind}:${origin ?? ''}:${id}` : '';
}

/** The loaded links, but only when they belong to the item shown now (`key`). */
export function visibleLinks<T>(links: T | null, loadedFor: string, key: string): T | null {
  return key && loadedFor === key ? links : null;
}
