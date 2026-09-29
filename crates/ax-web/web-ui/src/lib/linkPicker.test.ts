import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { isLinkTarget, isUrlQuery, wikiRaw, linkText, openLinkQuery, parseLinkQuery, searchLinkTargets, type LinkTarget } from './linkPicker.ts';

const item = (key: string, kind: LinkTarget['kind'], label: string, target: string, origin = 'project'): LinkTarget =>
  ({ key, kind, label, origin, target });

const ITEMS = [
  item('rule:project:review', 'rule', 'review', 'review'),
  item('rule:global:review', 'rule', 'review', 'global/rules/review', 'global'),
  item('skill:project:pr', 'skill', 'pr', 'pr'),
  item('memory:project:7', 'memory', 'Use SQLite', 'memories/Use SQLite'),
];

const filterLinkTargets = (items: LinkTarget[], query: string, selfKey?: string) =>
  searchLinkTargets(items, query, { selfKey }).items;

describe('searchLinkTargets, plain queries', () => {
  it('matches id or title case-insensitively', () => {
    assert.deepEqual(filterLinkTargets(ITEMS, 'sql').map((i) => i.key), ['memory:project:7']);
    assert.deepEqual(filterLinkTargets(ITEMS, 'REV').map((i) => i.key), ['rule:project:review', 'rule:global:review']);
    assert.deepEqual(filterLinkTargets(ITEMS, 'global/rules').map((i) => i.key), ['rule:global:review']);
  });

  it('matches a title that is not part of the link target', () => {
    const titled = [item('skill:project:pr', 'skill', 'Pull requests', 'pr')];
    assert.deepEqual(filterLinkTargets(titled, 'pull').map((i) => i.key), ['skill:project:pr']);
  });

  it('lists everything for an empty query, except the current item', () => {
    assert.deepEqual(filterLinkTargets(ITEMS, '', 'skill:project:pr').map((i) => i.key),
      ['rule:project:review', 'rule:global:review', 'memory:project:7']);
  });

  it('caps the list at LINK_PICKER_MAX (100) rows', () => {
    const many = Array.from({ length: 130 }, (_, n) => item(`rule:project:r${n}`, 'rule', `r${n}`, `r${n}`));
    assert.equal(filterLinkTargets(many, 'r').length, 100);
  });
});

describe('isLinkTarget', () => {
  it('accepts a graph node with every field', () => {
    assert.equal(isLinkTarget({ key: 'rule:project:a', kind: 'rule', label: 'a', origin: 'project', target: 'a', degree: 2 }), true);
  });

  it('rejects nodes with a missing or wrong field', () => {
    for (const bad of [
      null,
      'x',
      { key: 'k', kind: 'rule', label: 'a', origin: 'project' },
      { key: 'k', kind: 'turn', label: 'a', origin: 'project', target: 'a' },
      { key: 'k', kind: 'rule', label: 3, origin: 'project', target: 'a' },
      { key: 'k', kind: 'rule', label: 'a', origin: 'project', target: '' },
    ]) {
      assert.equal(isLinkTarget(bad), false, JSON.stringify(bad));
    }
  });
});

describe('linkText', () => {
  it('wraps the server target in double brackets', () => {
    assert.equal(linkText(ITEMS[1]), '[[global/rules/review]]');
    assert.equal(linkText(ITEMS[3]), '[[memories/Use SQLite]]');
  });
});

describe('openLinkQuery', () => {
  it('finds the query after an open [[', () => {
    assert.deepEqual(openLinkQuery('see [[rev', 9), { start: 4, query: 'rev' });
    assert.deepEqual(openLinkQuery('[[', 2), { start: 0, query: '' });
  });

  it('is null once the link is closed or crosses a line', () => {
    assert.equal(openLinkQuery('see [[rev]] x', 13), null);
    assert.equal(openLinkQuery('[[a\nb', 5), null);
    assert.equal(openLinkQuery('plain', 5), null);
  });
});

const tagged = (key: string, kind: LinkTarget['kind'], label: string, tags: string[]): LinkTarget =>
  ({ key, kind, label, origin: 'project', target: label, tags });

const MIX = [
  tagged('rule:project:azure-review', 'rule', 'azure-review', ['review', 'azure']),
  tagged('rule:project:english-only', 'rule', 'english-only', ['language']),
  tagged('skill:project:azdo-pr-review', 'skill', 'azdo-pr-review', ['azure', 'azdo', 'pr']),
  tagged('skill:project:review', 'skill', 'review', []),
  tagged('memory:project:9', 'memory', 'Azure CLI login fix', ['azcli']),
];
const keys = (r: { items: LinkTarget[] }) => r.items.map((i) => i.key);

describe('parseLinkQuery', () => {
  it('S1 splits plain words, lower-cased', () => {
    assert.deepEqual(parseLinkQuery('  Azure  Review '), { kind: null, words: ['azure', 'review'], tags: [] });
  });
  it('S2 reads #tag terms', () => {
    assert.deepEqual(parseLinkQuery('#Az pr'), { kind: null, words: ['pr'], tags: ['az'] });
  });
  it('S3 reads a kind prefix, long or short', () => {
    assert.equal(parseLinkQuery('skill:#azure').kind, 'skill');
    assert.equal(parseLinkQuery('s: x').kind, 'skill');
    assert.equal(parseLinkQuery('r:').kind, 'rule');
    assert.equal(parseLinkQuery('m:sql').kind, 'memory');
    assert.deepEqual(parseLinkQuery('m:sql').words, ['sql']);
    assert.equal(parseLinkQuery('rules are').kind, null);
  });
  it('a lone # is not a tag', () => {
    assert.deepEqual(parseLinkQuery('#'), { kind: null, words: [], tags: [] });
  });
});

describe('isUrlQuery', () => {
  it('B3 is true only for http(s) URLs', () => {
    assert.equal(isUrlQuery('https://example.com/x'), true);
    assert.equal(isUrlQuery(' http://a.b '), true);
    assert.equal(isUrlQuery('httpd'), false);
    assert.equal(isUrlQuery('review'), false);
  });
});

describe('searchLinkTargets', () => {
  it('P1 an empty query lists every kind, rules then skills then memory', () => {
    assert.deepEqual(keys(searchLinkTargets(MIX, '')), MIX.map((i) => i.key));
  });
  it('P1 an empty query is not cut to the first kind when the list is long', () => {
    const rules = Array.from({ length: 150 }, (_, n) => tagged(`rule:project:r${n}`, 'rule', `r${n}`, []));
    const r = searchLinkTargets([...rules, MIX[3], MIX[4]], '');
    assert.ok(r.items.some((i) => i.kind === 'skill'), 'a skill is shown');
    assert.ok(r.items.some((i) => i.kind === 'memory'), 'a memory is shown');
  });
  it('P2 the kind option narrows the list', () => {
    assert.deepEqual(keys(searchLinkTargets(MIX, '', { kind: 'skill' })), ['skill:project:azdo-pr-review', 'skill:project:review']);
  });
  it('P3 caps at 100 rows and reports the rest', () => {
    const many = Array.from({ length: 130 }, (_, n) => tagged(`rule:project:r${n}`, 'rule', `r${n}`, []));
    const r = searchLinkTargets(many, 'r');
    assert.equal(r.items.length, 100);
    assert.equal(r.more, 30);
  });
  it('P4 leaves out the item being edited', () => {
    assert.ok(!keys(searchLinkTargets(MIX, '', { selfKey: 'skill:project:review' })).includes('skill:project:review'));
  });
  it('S1 plain words match name, title or tags, and all must match', () => {
    assert.deepEqual(keys(searchLinkTargets(MIX, 'language')), ['rule:project:english-only']);
    assert.deepEqual(keys(searchLinkTargets(MIX, 'azure pr')), ['skill:project:azdo-pr-review']);
  });
  it('S2 #tag matches tags only, exact or prefix', () => {
    assert.deepEqual(keys(searchLinkTargets(MIX, '#az')).sort(),
      ['memory:project:9', 'rule:project:azure-review', 'skill:project:azdo-pr-review']);
    assert.deepEqual(keys(searchLinkTargets(MIX, '#review')), ['rule:project:azure-review']);
    assert.deepEqual(keys(searchLinkTargets(MIX, '#lang')), ['rule:project:english-only']);
  });
  it('S3 a kind prefix in the query filters like the chip', () => {
    assert.deepEqual(keys(searchLinkTargets(MIX, 's:#azure')), ['skill:project:azdo-pr-review']);
    assert.deepEqual(keys(searchLinkTargets(MIX, 'm:')), ['memory:project:9']);
  });
  it('S4 ranks exact name, then prefix, then contains, then tag or title only', () => {
    assert.deepEqual(keys(searchLinkTargets(MIX, 'review')),
      ['skill:project:review', 'rule:project:azure-review', 'skill:project:azdo-pr-review']);
    assert.deepEqual(keys(searchLinkTargets(MIX, 'az')),
      ['rule:project:azure-review', 'skill:project:azdo-pr-review', 'memory:project:9']);
    assert.deepEqual(keys(searchLinkTargets(MIX, 'azcli')), ['memory:project:9']);
  });
  it('treats missing tags as none', () => {
    assert.deepEqual(keys(searchLinkTargets(ITEMS, '#x')), []);
  });
});

describe('isLinkTarget tags', () => {
  it('accepts string tags and rejects other tag shapes', () => {
    const base = { key: 'k', kind: 'rule', label: 'a', origin: 'project', target: 'a' };
    assert.equal(isLinkTarget({ ...base, tags: ['x'] }), true);
    assert.equal(isLinkTarget({ ...base, tags: 'x' }), false);
    assert.equal(isLinkTarget({ ...base, tags: [1] }), false);
  });
});

describe('wikiRaw', () => {
  it('B2 uses the selection as the label', () => {
    assert.equal(wikiRaw(MIX[2], ''), 'azdo-pr-review');
    assert.equal(wikiRaw(MIX[2], '  the review skill '), 'azdo-pr-review|the review skill');
  });
  it('B2 drops characters that would break the link', () => {
    assert.equal(wikiRaw(MIX[2], 'a]]b|c\nd'), 'azdo-pr-review|a b c d');
  });
});

describe('searchLinkTargets counts', () => {
  it('P1 counts every match per kind, also past the cap', () => {
    const rules = Array.from({ length: 150 }, (_, n) => tagged(`rule:project:r${n}`, 'rule', `r${n}`, []));
    assert.deepEqual(searchLinkTargets([...rules, MIX[3]], '').counts, { rule: 150, skill: 1, memory: 0 });
  });
});

describe('ranking and URLs, edge cases', () => {
  it('S4 a name that starts with the word beats one that only contains it, across kinds', () => {
    const items = [
      tagged('rule:project:azdo-pr-review', 'rule', 'azdo-pr-review', []),
      tagged('skill:project:pr-tools', 'skill', 'pr-tools', []),
    ];
    assert.deepEqual(keys(searchLinkTargets(items, 'pr')), ['skill:project:pr-tools', 'rule:project:azdo-pr-review']);
  });
  it('B3 needs http:// or https:// and something after it', () => {
    assert.equal(isUrlQuery('https://'), false);
    assert.equal(isUrlQuery('http:foo'), false);
  });
});
