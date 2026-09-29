import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  RULE_PROPERTY_RESERVED,
  SKILL_PROPERTY_RESERVED,
  draftsFromProperties,
  formatPropertyValue,
  propertiesFromDrafts,
  reservedClash,
  type PropertyDraft,
} from './policyProperties.ts';

test('drafts keep text, zero, false, and lists', () => {
  const drafts = draftsFromProperties({
    files: ['src/a.rs', 'src/b.rs'],
    kind: 'review',
    level: 0,
    share: false,
  });
  assert.deepEqual(drafts, [
    { key: 'files', kind: 'list', text: 'src/a.rs, src/b.rs', checked: false },
    { key: 'kind', kind: 'text', text: 'review', checked: false },
    { key: 'level', kind: 'number', text: '0', checked: false },
    { key: 'share', kind: 'boolean', text: '', checked: false },
  ]);
});

test('blank text, blank lists, and blank keys are omitted; false and zero stay', () => {
  const drafts: PropertyDraft[] = [
    { key: '  ', kind: 'text', text: 'x', checked: false },
    { key: 'note', kind: 'text', text: '   ', checked: false },
    { key: 'files', kind: 'list', text: ' , ', checked: false },
    { key: 'count', kind: 'number', text: '0', checked: false },
    { key: 'count', kind: 'number', text: '', checked: false },
    { key: 'flag', kind: 'boolean', text: '', checked: false },
  ];
  assert.deepEqual(propertiesFromDrafts(drafts), { count: 0, flag: false });
});

test('a reserved built-in name is reported', () => {
  const drafts: PropertyDraft[] = [
    { key: 'tags', kind: 'text', text: 'nope', checked: false },
  ];
  assert.equal(reservedClash(drafts, RULE_PROPERTY_RESERVED), 'tags');
  assert.equal(reservedClash(drafts, SKILL_PROPERTY_RESERVED), 'tags');
  assert.equal(reservedClash([], RULE_PROPERTY_RESERVED), null);
});

test('a skill may keep globs as an extra property', () => {
  assert.equal(SKILL_PROPERTY_RESERVED.has('globs'), false);
  assert.equal(RULE_PROPERTY_RESERVED.has('globs'), true);
  assert.equal(RULE_PROPERTY_RESERVED.has('id'), true);
  assert.equal(SKILL_PROPERTY_RESERVED.has('name'), true);
  assert.equal(SKILL_PROPERTY_RESERVED.has('contextTask'), true);
});

test('formatPropertyValue renders each kind', () => {
  assert.equal(formatPropertyValue(false), 'No');
  assert.equal(formatPropertyValue(true), 'Yes');
  assert.equal(formatPropertyValue(0), '0');
  assert.equal(formatPropertyValue(['a', 'b']), 'a, b');
  assert.equal(formatPropertyValue(''), '');
});

test('a JSON object round-trips through a text draft', () => {
  const drafts = draftsFromProperties({ meta: { a: 1 } });
  assert.equal(drafts[0]?.text, '{"a":1}');
  assert.deepEqual(propertiesFromDrafts(drafts), { meta: { a: 1 } });
});
