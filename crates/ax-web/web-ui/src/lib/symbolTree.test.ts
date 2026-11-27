import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { NodeRow } from '../types.ts';
import { buildSymbolTree, sectionLabelForKind, shouldShowSectionHeader, type SymbolTreeNode } from './symbolTree.ts';

function node(id: string, kind: string, start: number, end: number, name = id): NodeRow {
  return { id, kind, name, qualified_name: name, file_path: 'a.rs', language: 'rust', start_line: start, end_line: end, signature: null, is_exported: 1 };
}

function shape(tree: SymbolTreeNode[]): unknown[] {
  return tree.map((t) => (t.children.length ? { [t.node.id]: shape(t.children) } : t.node.id));
}

test('an empty file has no tree', () => {
  assert.deepEqual(buildSymbolTree([]), []);
});

test('symbols nest by line containment under the file and sort by kind, then name', () => {
  const tree = buildSymbolTree([
    node('file', 'file', 1, 100),
    node('zeta', 'function', 80, 90),
    node('Store', 'struct', 10, 40),
    node('get', 'method', 20, 25),
    node('alpha', 'function', 50, 60),
    node('put', 'method', 30, 35),
  ]);
  assert.deepEqual(shape(tree), [{ Store: ['get', 'put'] }, 'alpha', 'zeta']);
});

test('the smallest enclosing symbol is the parent', () => {
  const tree = buildSymbolTree([
    node('file', 'file', 1, 100),
    node('outer', 'module', 2, 90),
    node('Inner', 'class', 5, 50),
    node('run', 'method', 10, 12),
  ]);
  assert.deepEqual(shape(tree), [{ outer: [{ Inner: ['run'] }] }]);
});

test('a symbol with the same span as its container nests under the higher-ranked kind', () => {
  const tree = buildSymbolTree([node('file', 'file', 1, 10), node('M', 'module', 1, 10), node('C', 'class', 1, 10)]);
  assert.deepEqual(shape(tree), [{ M: ['C'] }]);
});

test('without a file node the top-level symbols are roots', () => {
  const tree = buildSymbolTree([node('Store', 'struct', 1, 10), node('get', 'method', 2, 3)]);
  assert.deepEqual(shape(tree), [{ Store: ['get'] }]);
});

test('section headers appear where the kind changes', () => {
  const tree = buildSymbolTree([
    node('file', 'file', 1, 100),
    node('A', 'struct', 1, 2),
    node('B', 'struct', 3, 4),
    node('f', 'function', 5, 6),
  ]);
  assert.deepEqual(tree.map((_, i) => shouldShowSectionHeader(tree, i)), [true, false, true]);
  assert.equal(sectionLabelForKind('struct'), 'Structs');
  assert.equal(sectionLabelForKind('macro'), 'macro');
});
