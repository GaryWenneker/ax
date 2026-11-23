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
