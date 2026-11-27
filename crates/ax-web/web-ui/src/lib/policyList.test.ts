import assert from 'node:assert/strict';
import test from 'node:test';
import { addPolicyEntries, cleanPolicyList, insideBraces, pasteIntoDraft, splitPolicyList } from './policyList.ts';

test('a brace glob stays one entry', () => {
  assert.deepEqual(splitPolicyList('src/**/*.{ts,tsx}, **/*.rs'), ['src/**/*.{ts,tsx}', '**/*.rs']);
});

test('nested braces stay one entry', () => {
  assert.deepEqual(splitPolicyList('a/{b,{c,d}}/*.md,e'), ['a/{b,{c,d}}/*.md', 'e']);
});

test('plain commas split and blanks are dropped', () => {
  assert.deepEqual(splitPolicyList(' mobile , deploy,, '), ['mobile', 'deploy']);
  assert.deepEqual(splitPolicyList(''), []);
});

test('a stray closing brace does not swallow later commas', () => {
  assert.deepEqual(splitPolicyList('a}, b'), ['a}', 'b']);
});

test('joining and splitting round-trips brace globs', () => {
  const globs = ['src/**/*.{ts,tsx}', 'lib/*.{js,mjs,cjs}', '**/*.rs'];
  assert.deepEqual(splitPolicyList(globs.join(', ')), globs);
});

test('insideBraces is true only while a brace is open', () => {
  assert.equal(insideBraces('*.{ts'), true);
  assert.equal(insideBraces('*.{ts,tsx}'), false);
  assert.equal(insideBraces('plain'), false);
});

test('cleanPolicyList keeps commas inside entries', () => {
  assert.deepEqual(cleanPolicyList([' a,b ', '', 'c']), ['a,b', 'c']);
});

test('pasted comma-separated text adds one chip per entry', () => {
  assert.deepEqual(addPolicyEntries(['deploy'], 'react, frontend'), ['deploy', 'react', 'frontend']);
});

test('pasted brace globs stay whole and duplicates are skipped without regard to case', () => {
  assert.deepEqual(
    addPolicyEntries(['**/*.rs'], 'src/**/*.{ts,tsx}, **/*.RS, src/**/*.{ts,tsx}'),
    ['**/*.rs', 'src/**/*.{ts,tsx}'],
  );
  assert.deepEqual(addPolicyEntries(['a'], ' , '), ['a']);
});

test('a paste over a selected draft replaces the selection', () => {
  assert.equal(pasteIntoDraft('foo', 'a, b', 0, 3), 'a, b');
  assert.deepEqual(splitPolicyList(pasteIntoDraft('foo', 'a, b', 0, 3)), ['a', 'b']);
});

test('a paste goes in at the caret', () => {
  assert.equal(pasteIntoDraft('src/*.rs', 'x,y', 4, 4), 'src/x,y*.rs');
  assert.equal(pasteIntoDraft('ab', 'c, d', 2, 2), 'abc, d');
  assert.equal(pasteIntoDraft('abcd', 'x, y', 1, 3), 'ax, yd');
});

test('a paste with no known caret goes at the end of the draft', () => {
  assert.equal(pasteIntoDraft('foo', ', a', null, null), 'foo, a');
  assert.equal(pasteIntoDraft('foo', ', a', 1, null), 'f, aoo');
});

test('a paste with a reversed or out-of-range selection stays inside the draft', () => {
  assert.equal(pasteIntoDraft('abc', 'x', 3, 1), 'ax');
  assert.equal(pasteIntoDraft('abc', 'x', 9, 9), 'abcx');
});
