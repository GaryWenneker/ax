export function memoryTierLabel(tier: string | undefined): string {
  switch (tier) {
    case 't0':
      return 'working';
    case 't1':
      return 'recent';
    case 't3':
      return 'archive';
    default:
      return 'long-term';
  }
}

export function recalledFromArchiveLabel(
  tier: string | undefined,
  archiveSearch: boolean,
): string | null {
  if (archiveSearch && tier === 't3') return 'recalled from archive';
  return null;
}
