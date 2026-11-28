import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fetchBrowseTree } from './fileTree.ts';

type F = { path: string };

test('a tree refresh reloads the roots and every expanded folder in one result', async () => {
  const asked: string[] = [];
  const tree = await fetchBrowseTree<F, { count: number }>(['', 'b/', 'a/'], {
    fetchRoots: async () => ({ roots: [{ count: 3 }, { count: 4 }], files: [{ path: 'z.md' }] }),
    fetchPrefix: async (prefix) => {
      asked.push(prefix);
      return { files: [{ path: `${prefix}one.ts` }, { path: 'z.md' }] };
    },
  });
  assert.deepEqual(asked.sort(), ['a/', 'b/'], 'the root prefix comes from the roots call');
  assert.deepEqual(tree.files.map((f) => f.path), ['a/one.ts', 'b/one.ts', 'z.md']);
  assert.deepEqual([...tree.loaded].sort(), ['', 'a/', 'b/']);
  assert.equal(tree.total, 8);
});

test('a folder that fails to load is dropped from the loaded set instead of failing the refresh', async () => {
  const tree = await fetchBrowseTree<F, { count: number }>(['gone/', 'kept/'], {
    fetchRoots: async () => ({ roots: [], files: [] }),
    fetchPrefix: async (prefix) => {
      if (prefix === 'gone/') throw new Error('404');
      return { files: [{ path: 'kept/x' }] };
    },
  });
  assert.deepEqual([...tree.loaded].sort(), ['', 'kept/']);
  assert.deepEqual(tree.files.map((f) => f.path), ['kept/x']);
});
