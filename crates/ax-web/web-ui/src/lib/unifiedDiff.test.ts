import assert from 'node:assert/strict';
import test from 'node:test';
import { formatUnifiedDiff, unifiedDiff } from './unifiedDiff.ts';

test('a one-line change keeps order and three lines of context', () => {
  const text = formatUnifiedDiff(unifiedDiff('a\nb\nc\n', 'a\nB\nc\n'));
  assert.match(text, /@@ -1,3 \+1,3 @@/);
  assert.match(text, /^ a$/m);
  assert.match(text, /^-b$/m);
  assert.match(text, /^\+B$/m);
  assert.match(text, /^ c$/m);
});

test('identical text has no hunks', () => {
  assert.deepEqual(unifiedDiff('same\n', 'same\n'), []);
});

test('a blank line is part of the diff', () => {
  const text = formatUnifiedDiff(unifiedDiff('a\n\nb\n', 'a\n\nc\n'));
  assert.match(text, /^-b$/m);
  assert.match(text, /^\+c$/m);
  assert.match(text, /^ $/m);
});

function numbered(count: number, prefix: string): string[] {
  return Array.from({ length: count }, (_, index) => `${prefix} line ${index + 1}`);
}

test('hunk positions are right for several separate changes', () => {
  const before = numbered(30, 'x');
  const after = [...before];
  after[4] = 'changed five';
  after[24] = 'changed twenty-five';
  const hunks = unifiedDiff(`${before.join('\n')}\n`, `${after.join('\n')}\n`);
  assert.deepEqual(hunks.map((h) => [h.oldStart, h.oldCount, h.newStart, h.newCount]), [[2, 7, 2, 7], [22, 7, 22, 7]]);
  assert.deepEqual(hunks[1].lines.filter((l) => l.kind !== 'context').map((l) => l.text), ['x line 25', 'changed twenty-five']);
});

test('one change in a very long document is found quickly', () => {
  const before = numbered(6000, 'spec');
  const after = [...before];
  after[3000] = 'edited';
  const started = performance.now();
  const hunks = unifiedDiff(before.join('\n'), after.join('\n'));
  const elapsed = performance.now() - started;
  assert.equal(hunks.length, 1);
  assert.equal(hunks[0].oldStart, 2998);
  assert.deepEqual(hunks[0].lines.filter((l) => l.kind !== 'context').map((l) => l.text), ['spec line 3001', 'edited']);
  assert.ok(elapsed < 250, `took ${elapsed.toFixed(0)} ms`);
});

test('two unrelated very long documents give one whole replace hunk without a huge table', () => {
  const before = numbered(6000, 'old');
  const after = numbered(6000, 'new');
  const started = performance.now();
  const hunks = unifiedDiff(before.join('\n'), after.join('\n'));
  const elapsed = performance.now() - started;
  assert.equal(hunks.length, 1);
  assert.deepEqual([hunks[0].oldStart, hunks[0].oldCount, hunks[0].newStart, hunks[0].newCount], [1, 6000, 1, 6000]);
  assert.deepEqual(hunks[0].lines.slice(0, 1), [{ kind: 'remove', text: 'old line 1' }]);
  assert.deepEqual(hunks[0].lines.slice(-1), [{ kind: 'add', text: 'new line 6000' }]);
  assert.ok(elapsed < 250, `took ${elapsed.toFixed(0)} ms`);
});
