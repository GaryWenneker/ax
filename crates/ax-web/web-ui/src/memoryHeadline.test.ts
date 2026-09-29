import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { memoryRowAge, memoryRowHeadline } from './memoryHeadline.ts';

describe('memory row headline', () => {
  it('uses the longer first body line when the stored title is a prefix of it', () => {
    const title = 'als je op het azure devops board kijkt, zie je dat ik meer tickets open heb staa';
    const full = `${title}n voor de git message.`;
    assert.equal(memoryRowHeadline({ title, body: `${full}\n\nFiles: a.rs` }), full);
  });

  it('keeps the title when the body starts with something else', () => {
    assert.equal(
      memoryRowHeadline({ title: 'Cache key', body: 'Invalidate when the generation changes.' }),
      'Cache key',
    );
  });

  it('keeps the title when the first line is not longer', () => {
    assert.equal(memoryRowHeadline({ title: 'Cache key', body: 'Cache key\n\nMore.' }), 'Cache key');
  });

  it('skips blank lines before the first line', () => {
    assert.equal(memoryRowHeadline({ title: 'Hel', body: '\n\n  Hello there\n' }), 'Hello there');
  });

  it('returns the title when the body is empty', () => {
    assert.equal(memoryRowHeadline({ title: 'Note', body: '   \n' }), 'Note');
  });
});

describe('memory row age', () => {
  const now = 1_700_000_000_000;

  it('says just now for a few seconds and for a future timestamp', () => {
    assert.equal(memoryRowAge(now - 10_000, now), 'just now');
    assert.equal(memoryRowAge(now + 5_000, now), 'just now');
  });

  it('uses minutes, then hours, then days', () => {
    assert.equal(memoryRowAge(now - 5 * 60_000, now), '5m ago');
    assert.equal(memoryRowAge(now - 18 * 3_600_000, now), '18h ago');
    assert.equal(memoryRowAge(now - 3 * 86_400_000, now), '3d ago');
  });
});
