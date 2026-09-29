import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MEMORY_CATEGORIES, memoryCategory } from './memoryCategory.ts';

test('every known kind maps to its category', () => {
  const cases: [string, string][] = [
    ['git', 'commit'],
    ['turn', 'turn'],
    ['doc', 'doc'],
    ['decision', 'decision'],
    ['architecture', 'decision'],
    ['convention', 'convention'],
    ['fix', 'fix'],
    ['bug_fix', 'fix'],
    ['note', 'note'],
    ['something-new', 'note'],
    ['', 'note'],
  ];
  for (const [kind, cat] of cases) assert.equal(memoryCategory(kind), cat, kind);
});

test('categories list their labels in legend order', () => {
  assert.deepEqual(
    MEMORY_CATEGORIES.map((c) => [c.id, c.label]),
    [
      ['commit', 'Commit'],
      ['turn', 'Chat turn'],
      ['doc', 'Folder doc'],
      ['decision', 'Decision'],
      ['convention', 'Convention'],
      ['fix', 'Fix'],
      ['note', 'Note'],
    ],
  );
});
