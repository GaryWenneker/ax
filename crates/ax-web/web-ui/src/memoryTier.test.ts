import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { memoryTierLabel, recalledFromArchiveLabel } from './memoryTier.ts';

describe('memory tier label', () => {
  it('names each stored tier in words', () => {
    assert.equal(memoryTierLabel('t1'), 'recent');
    assert.equal(memoryTierLabel('t2'), 'long-term');
    assert.equal(memoryTierLabel('t3'), 'archive');
    assert.equal(memoryTierLabel(undefined), 'long-term');
  });

  it('says recalled from archive only for an archive search hit', () => {
    assert.equal(recalledFromArchiveLabel('t3', true), 'recalled from archive');
    assert.equal(recalledFromArchiveLabel('t3', false), null);
    assert.equal(recalledFromArchiveLabel('t2', true), null);
  });
});
