export type MemoryCategory = 'commit' | 'turn' | 'doc' | 'decision' | 'convention' | 'fix' | 'note';

export const MEMORY_CATEGORIES: { id: MemoryCategory; label: string }[] = [
  { id: 'commit', label: 'Commit' },
  { id: 'turn', label: 'Chat turn' },
  { id: 'doc', label: 'Folder doc' },
  { id: 'decision', label: 'Decision' },
  { id: 'convention', label: 'Convention' },
  { id: 'fix', label: 'Fix' },
  { id: 'note', label: 'Note' },
];

const BY_KIND: Record<string, MemoryCategory> = {
  git: 'commit',
  turn: 'turn',
  doc: 'doc',
  decision: 'decision',
  architecture: 'decision',
  convention: 'convention',
  fix: 'fix',
  bug_fix: 'fix',
};

export function memoryCategory(kind: string): MemoryCategory {
  return BY_KIND[kind] ?? 'note';
}
