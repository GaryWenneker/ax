import assert from 'node:assert/strict';
import { test } from 'node:test';
import { frameStateAfter } from './sonarFrame.ts';

test('the frame finishing its load does not hide a failed status probe', () => {
  assert.equal(frameStateAfter('error', 'load'), 'error');
});

test('a frame that loads while still loading is shown', () => {
  assert.equal(frameStateAfter('loading', 'load'), 'loaded');
});

test('a frame error always shows the offline panel', () => {
  assert.equal(frameStateAfter('loading', 'error'), 'error');
  assert.equal(frameStateAfter('loaded', 'error'), 'error');
});
