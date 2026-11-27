import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { FOCUSABLE_SELECTOR, withDescribedBy, withoutDescribedBy } from './hoverTip.ts';

test('the focusable selector covers the interactive children HoverTip wraps', () => {
  for (const part of ['button', 'a[href]', 'input', 'select', 'textarea', '[tabindex]']) {
    assert.ok(FOCUSABLE_SELECTOR.includes(part), part);
  }
});

test('describedby tokens are added once and removed without touching others', () => {
  assert.equal(withDescribedBy(null, 't1'), 't1');
  assert.equal(withDescribedBy('help', 't1'), 'help t1');
  assert.equal(withDescribedBy('help t1', 't1'), 'help t1');
  assert.equal(withoutDescribedBy('help t1', 't1'), 'help');
  assert.equal(withoutDescribedBy('t1', 't1'), null);
  assert.equal(withoutDescribedBy(null, 't1'), null);
});

test('HoverTip takes a tab stop only when its child has none and ties the tip to the control', () => {
  const source = readFileSync(new URL('../components/HoverTip.tsx', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /tabIndex=\{0\}/);
  assert.match(source, /tabIndex=\{ownFocus \? 0 : undefined\}/);
  assert.match(source, /querySelector<HTMLElement>\(FOCUSABLE_SELECTOR\)/);
  assert.match(source, /id=\{tipId\}/);
});
