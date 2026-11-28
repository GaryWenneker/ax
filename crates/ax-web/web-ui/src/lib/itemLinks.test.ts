import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { itemLinksKey, visibleLinks } from './itemLinks.ts';

const linksA = { outgoing: [], backlinks: [] };

test('links loaded for another item are not shown while the new item loads', () => {
  const keyA = itemLinksKey('rule', 'a', undefined);
  const keyB = itemLinksKey('rule', 'b', undefined);
  assert.equal(visibleLinks(linksA, keyA, keyA), linksA);
  assert.equal(visibleLinks(linksA, keyA, keyB), null);
  assert.equal(visibleLinks(linksA, keyA, ''), null);
});

test('the item key includes kind, origin and id; no id means no key', () => {
  assert.equal(itemLinksKey('skill', 'pr', 'global'), 'skill:global:pr');
  assert.equal(itemLinksKey('rule', 'x', undefined), 'rule::x');
  assert.equal(itemLinksKey('rule', null, 'global'), '');
});

test('inline workspaces refetch links after a save', () => {
  const hook = readFileSync(new URL('../components/LinkedBody.tsx', import.meta.url), 'utf8');
  assert.match(hook, /\}, \[key, body, version\]\);/);
  assert.match(hook, /return visibleLinks\(links, loadedFor, key\);/);
  for (const [file, kind] of [['PolicyRuleInlineWorkspace', 'rule'], ['PolicySkillInlineWorkspace', 'skill']]) {
    const source = readFileSync(new URL(`../components/${file}.tsx`, import.meta.url), 'utf8');
    assert.match(source, new RegExp(`useItemLinks\\('${kind}', \\w+, origin, body, savedTick\\)`), file);
    assert.match(source, /setSavedTick\(\(t\) => t \+ 1\)/, file);
  }
});
