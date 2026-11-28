import assert from 'node:assert/strict';
import { test } from 'node:test';
import { AS_OF_WINDOW_MS, asOfFromSlider, graphSource, sliderFromAsOf } from './graphAsOf.ts';

const NOW = 1_800_000_000_000;

test('the slider at 100 is the live graph and below it a time in the last 90 days', () => {
  assert.equal(asOfFromSlider(100, NOW), null);
  assert.equal(asOfFromSlider(0, NOW), NOW - AS_OF_WINDOW_MS);
  assert.equal(asOfFromSlider(50, NOW), NOW - AS_OF_WINDOW_MS / 2);
  assert.equal(sliderFromAsOf(null, NOW), 100);
  assert.equal(sliderFromAsOf(NOW - AS_OF_WINDOW_MS / 2, NOW), 50);
});

test('clearing the as-of time returns to the live graph', () => {
  assert.equal(graphSource({ asOfMs: NOW - 1000, viewMode: 'structure', system: '' }), 'as-of');
  assert.equal(graphSource({ asOfMs: null, viewMode: 'structure', system: '' }), 'live');
});

test('the as-of snapshot is local only: domain mode and a replica system load their own view', () => {
  assert.equal(graphSource({ asOfMs: NOW - 1000, viewMode: 'domain', system: '' }), 'domain');
  assert.equal(graphSource({ asOfMs: NOW - 1000, viewMode: 'structure', system: 'peer-1' }), 'live');
});

test('live changes merge only into the live graph, never into a snapshot', async () => {
  const { acceptsLiveMerge } = await import('./graphAsOf.ts');
  assert.equal(acceptsLiveMerge({ asOfMs: null, viewMode: 'structure', system: '' }), true);
  assert.equal(acceptsLiveMerge({ asOfMs: NOW - 1000, viewMode: 'structure', system: '' }), false);
  assert.equal(acceptsLiveMerge({ asOfMs: null, viewMode: 'domain', system: '' }), false);
});
