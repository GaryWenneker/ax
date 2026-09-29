import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { clampPolicyListWidth, loadBodyView, saveBodyView } from './policyBodyView.ts';

function memoryStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => { data.set(k, v); },
  };
}

describe('policy body view', () => {
  it('opens in preview when nothing is saved', () => {
    assert.equal(loadBodyView(memoryStorage()), 'preview');
  });

  it('returns the saved view', () => {
    assert.equal(loadBodyView(memoryStorage({ 'ax-web-policy-body-view': 'markdown' })), 'markdown');
    assert.equal(loadBodyView(memoryStorage({ 'ax-web-policy-body-view': 'preview' })), 'preview');
    assert.equal(loadBodyView(memoryStorage({ 'ax-web-policy-body-view': 'wysiwyg' })), 'wysiwyg');
  });

  it('falls back to preview for a junk value', () => {
    assert.equal(loadBodyView(memoryStorage({ 'ax-web-policy-body-view': 'split' })), 'preview');
  });

  it('remembers a saved choice', () => {
    const storage = memoryStorage();
    saveBodyView('markdown', storage);
    assert.equal(loadBodyView(storage), 'markdown');
  });
});

describe('policy list width', () => {
  it('raises narrow widths to the minimum', () => {
    assert.equal(clampPolicyListWidth(200), 240);
    assert.equal(clampPolicyListWidth(100), 240);
  });

  it('keeps widths in range and caps wide ones', () => {
    assert.equal(clampPolicyListWidth(300), 300);
    assert.equal(clampPolicyListWidth(900), 480);
  });

  it('uses the default for a non-number', () => {
    assert.equal(clampPolicyListWidth(Number.NaN), 280);
  });
});
