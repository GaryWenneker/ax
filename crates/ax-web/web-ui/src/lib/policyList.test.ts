import assert from 'node:assert/strict';
import test from 'node:test';
import { addPolicyEntries, cleanPolicyList, insideBraces, splitPolicyList } from './policyList.ts';

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
