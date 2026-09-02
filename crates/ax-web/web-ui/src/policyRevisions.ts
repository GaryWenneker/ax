export function revisionSourceLabel(source: string): string {
  if (source === 'restore') return 'Package restore';
  if (source === 'save') return 'Save';
  return source;
}

export function revisionHashPrefix(hash: string): string {
  return hash.slice(0, 12);
}
