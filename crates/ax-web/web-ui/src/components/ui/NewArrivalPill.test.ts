import { describe, expect, it } from 'vitest';
import { arrivalLabel, nextPending } from './NewArrivalPill';

describe('nextPending', () => {
  it('counts a live batch only when the view is away from the newest rows', () => {
    expect(nextPending(false, true, 3, 1)).toBe(4);
  });

  it('leaves the count unchanged while following, for history, and for an empty batch', () => {
    expect(nextPending(true, true, 3, 1)).toBe(1);
    expect(nextPending(false, false, 3, 1)).toBe(1);
    expect(nextPending(false, true, 0, 1)).toBe(1);
  });
});

describe('arrivalLabel', () => {
  it('singularizes one arrival and pluralizes the rest', () => {
    expect(arrivalLabel(1, 'event')).toBe('1 new event');
    expect(arrivalLabel(2, 'event')).toBe('2 new events');
    expect(arrivalLabel(4, 'item')).toBe('4 new items');
  });
});
