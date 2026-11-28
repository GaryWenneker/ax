import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { arrivalLabel, nextPending } from './newArrival.ts';

describe('nextPending', () => {
  it('counts a live batch only when the view is away from the newest rows', () => {
    assert.equal(nextPending(false, true, 3, 1), 4);
  });

  it('leaves the count unchanged while following, for history, and for an empty batch', () => {
    assert.equal(nextPending(true, true, 3, 1), 1);
    assert.equal(nextPending(false, false, 3, 1), 1);
    assert.equal(nextPending(false, true, 0, 1), 1);
  });
});

describe('arrivalLabel', () => {
  it('singularizes one arrival and pluralizes the rest', () => {
    assert.equal(arrivalLabel(1, 'event'), '1 new event');
    assert.equal(arrivalLabel(2, 'event'), '2 new events');
    assert.equal(arrivalLabel(4, 'item'), '4 new items');
  });
});
