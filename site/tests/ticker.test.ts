import assert from 'node:assert/strict';
import test from 'node:test';
import { tickerCopiesNeeded, tickerDurationSeconds, tickerHalfCopies } from '../src/lib/ticker.ts';

test('a short sequence is repeated until the row is wider than the screen plus one loop', () => {
	assert.equal(tickerCopiesNeeded(1000, 3000, 2), 4);
	assert.equal(tickerCopiesNeeded(500, 4000, 1), 9);
});

test('an already long row is not shortened', () => {
	assert.equal(tickerCopiesNeeded(2000, 1000, 8), 8);
});

test('a missing measurement keeps the copies already rendered', () => {
	assert.equal(tickerCopiesNeeded(0, 3000, 8), 8);
	assert.equal(tickerCopiesNeeded(1000, 0, 8), 8);
});

test('each marquee half covers the screen so a -50% loop has no gap', () => {
	assert.equal(tickerHalfCopies(1000, 3000), 3);
	assert.equal(tickerHalfCopies(500, 4000), 8);
	assert.equal(tickerHalfCopies(2000, 1000), 1);
	assert.equal(tickerHalfCopies(0, 3000), 1);
	assert.equal(tickerHalfCopies(1000, 0), 1);
});

test('the loop duration tracks the sequence width and never flashes past', () => {
	assert.equal(tickerDurationSeconds(1400), 20);
	assert.equal(tickerDurationSeconds(100), 12);
});
