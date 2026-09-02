/** Group-folder filter + collapse-all helpers (UI list only). */

/** No selection means “all groups”. Otherwise the item’s resolved group must be listed. */
export function matchesGroupFilter(groupId: string, selectedIds: readonly string[]): boolean {
  if (selectedIds.length === 0) return true;
  return selectedIds.includes(groupId);
}

export function toggleGroupFilterId(selected: readonly string[], id: string): string[] {
  return selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id];
}

export function collapseAllGroupIds(ids: readonly string[]): Set<string> {
  return new Set(ids);
}

export function expandAllGroupIds(): Set<string> {
  return new Set();
}

export function allListedCollapsed(listedIds: readonly string[], collapsed: ReadonlySet<string>): boolean {
  return listedIds.length === 0 || listedIds.every((id) => collapsed.has(id));
}

export function allListedExpanded(listedIds: readonly string[], collapsed: ReadonlySet<string>): boolean {
  return listedIds.every((id) => !collapsed.has(id));
}
