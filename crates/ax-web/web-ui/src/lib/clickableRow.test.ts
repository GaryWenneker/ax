import assert from 'node:assert/strict';
import { test } from 'node:test';
import { clickableRowProps } from './clickableRow.ts';

function keyEvent(key: string, onSelf = true) {
  const row = {};
  let prevented = false;
  return {
    event: {
      key,
      currentTarget: row,
      target: onSelf ? row : {},
      preventDefault: () => { prevented = true; },
    },
    prevented: () => prevented,
  };
}

test('a clickable row can be reached with Tab', () => {
  const props = clickableRowProps(() => {});
  assert.equal(props.tabIndex, 0);
});

test('Enter and Space on a focused row do what a click does', () => {
  let hits = 0;
  const props = clickableRowProps(() => { hits += 1; });
  for (const key of ['Enter', ' ']) {
    const k = keyEvent(key);
    props.onKeyDown(k.event as never);
    assert.equal(k.prevented(), true, `${JSON.stringify(key)} must not scroll the page`);
  }
  props.onClick();
  assert.equal(hits, 3);
});

test('other keys and keys pressed on a control inside the row are left alone', () => {
  let hits = 0;
  const props = clickableRowProps(() => { hits += 1; });
  props.onKeyDown(keyEvent('a').event as never);
  props.onKeyDown(keyEvent('Enter', false).event as never);
  assert.equal(hits, 0);
});

test('the selected state is exposed to assistive technology when given', () => {
  assert.equal(clickableRowProps(() => {}, { selected: true })['aria-selected'], true);
  assert.equal(clickableRowProps(() => {}, { selected: false })['aria-selected'], false);
  assert.equal('aria-selected' in clickableRowProps(() => {}), false);
});
