import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  RULE_PROPERTY_RESERVED,
  SKILL_PROPERTY_RESERVED,
  draftsFromProperties,
  formatPropertyValue,
  invalidNumber,
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
  assert.deepEqual(drafts.map(({ original: _original, ...row }) => row), [
    { key: 'files', kind: 'list', text: 'src/a.rs, src/b.rs', checked: false },
    { key: 'kind', kind: 'text', text: 'review', checked: false },
    { key: 'level', kind: 'number', text: '0', checked: false },
    { key: 'share', kind: 'boolean', text: '', checked: false },
  ]);
  assert.deepEqual(drafts.map((row) => row.original?.value), [['src/a.rs', 'src/b.rs'], 'review', 0, false]);
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

function editKey(drafts: PropertyDraft[], key: string, patch: Partial<PropertyDraft>): PropertyDraft[] {
  return drafts.map((row) => (row.key === key ? { ...row, ...patch } : row));
}

test('untouched rows keep their exact value when another row is added', () => {
  const original = {
    ports: [8080, 9090],
    owner: null,
    flags: [true, false],
    pairs: ['a, b', 'c'],
    empty: [],
    note: '{"looks":"like json"}',
    meta: { a: 1 },
  };
  const drafts = [...draftsFromProperties(original), { key: 'team', kind: 'text' as const, text: 'core', checked: false }];
  assert.deepEqual(propertiesFromDrafts(drafts), { ...original, team: 'core' });
});

test('an edited list keeps the element type of the original list', () => {
  const drafts = editKey(draftsFromProperties({ ports: [8080, 9090] }), 'ports', { text: '8080, 9090, 7070' });
  assert.deepEqual(propertiesFromDrafts(drafts), { ports: [8080, 9090, 7070] });
  const flags = editKey(draftsFromProperties({ flags: [true] }), 'flags', { text: 'true, false' });
  assert.deepEqual(propertiesFromDrafts(flags), { flags: [true, false] });
});

test('an edited text that was a string stays a string even when it looks like JSON', () => {
  const drafts = editKey(draftsFromProperties({ note: 'x' }), 'note', { text: '{"a":1}' });
  assert.deepEqual(propertiesFromDrafts(drafts), { note: '{"a":1}' });
});

test('an invalid number is reported and keeps the stored value', () => {
  const drafts = editKey(draftsFromProperties({ port: 80 }), 'port', { text: '1O' });
  assert.deepEqual(propertiesFromDrafts(drafts), { port: 80 });
  assert.equal(invalidNumber(drafts), 'port');
  assert.equal(invalidNumber(draftsFromProperties({ port: 80 })), null);
  assert.equal(invalidNumber([{ key: 'fresh', kind: 'number', text: '', checked: false }]), null);
});
