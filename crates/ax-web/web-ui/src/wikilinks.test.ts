import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import {
  linkHref,
  MISSING_HREF,
  splitWikiLinks,
  wikiLinksToMarkdown,
  type OutgoingLink,
} from './wikilinks.ts';

function link(text: string, target: string, resolved: OutgoingLink['resolved'], label: string | null = null): OutgoingLink {
  return { text, target, heading: null, label, resolved };
}

describe('splitWikiLinks', () => {
  it('splits text and links, keeping every character', () => {
    const body = 'Use [[pr]] and ![[memories/Use SQLite|the note]].\nNext line';
    const parts = splitWikiLinks(body);
    assert.deepEqual(parts, [
      { text: 'Use ' },
      { link: '[[pr]]' },
      { text: ' and ' },
      { link: '![[memories/Use SQLite|the note]]' },
      { text: '.\nNext line' },
    ]);
    assert.equal(parts.map((p) => ('link' in p ? p.link : p.text)).join(''), body);
  });

  it('U3: links in fenced code and inline code stay text', () => {
    const body = 'a `[[x]]` b ``[[y]]`` c\n```\n[[z]]\n```\n~~~~\n[[w]]\n~~~\n[[w2]]\n~~~~\n[[ok]]';
    const links = splitWikiLinks(body).filter((p) => 'link' in p);
    assert.deepEqual(links, [{ link: '[[ok]]' }]);
  });

  it('matches the server on odd input', () => {
    const odd = ['[[]]', '[[ ]]', '[[a[[b]]', '[[unclosed', 'x `[[a]]', '[[#h]]', '[[a\nb]]'];
    for (const body of odd) {
      const links = splitWikiLinks(body).filter((p) => 'link' in p);
      const expected = body === 'x `[[a]]' ? [{ link: '[[a]]' }] : [];
      assert.deepEqual(links, expected, body);
    }
  });
});

describe('linkHref', () => {
  it('opens rules, skills and memories in the Command Center', () => {
    assert.equal(linkHref({ kind: 'rule', id: 'english-only', origin: 'project' }), '/policy/rules?id=english-only');
    assert.equal(linkHref({ kind: 'skill', id: 'pr', origin: 'project' }), '/policy/skills?name=pr');
    assert.equal(linkHref({ kind: 'memory', id: 'a b', origin: 'project' }), '/memory?id=a+b');
    assert.equal(
      linkHref({ kind: 'skill', id: 'review-loop', origin: 'global', projectId: 7 }),
      '/policy/skills?name=review-loop&origin=global&projectId=7',
    );
  });
});

describe('wikiLinksToMarkdown', () => {
  it('U1: resolved links become Markdown links with their label or name', () => {
    const out = wikiLinksToMarkdown('See [[pr]] and [[rules/x|the rule]].', [
      link('[[pr]]', 'pr', { kind: 'skill', id: 'pr', origin: 'project' }),
      link('[[rules/x|the rule]]', 'rules/x', { kind: 'rule', id: 'x', origin: 'project' }, 'the rule'),
    ]);
    assert.equal(out, 'See [pr](/policy/skills?name=pr) and [the rule](/policy/rules?id=x).');
  });

  it('U2: a missing link gets the missing style and tooltip', () => {
    const out = wikiLinksToMarkdown('Try [[nope]].', [link('[[nope]]', 'nope', null)]);
    assert.equal(out, `Try [nope](${MISSING_HREF} "No rule, skill or memory named nope").`);
  });

  it('escapes brackets and quotes so the Markdown stays one link', () => {
    const out = wikiLinksToMarkdown('[[a "b"|x ] y]]', [link('[[a "b"|x ] y]]', 'a "b"', null, 'x ] y')]);
    assert.equal(out, `[x \\] y](${MISSING_HREF} "No rule, skill or memory named a \\"b\\"")`);
  });

  it('U3 and N4: code and unknown links are left exactly as written', () => {
    const body = '`[[pr]]` and [[pr]] and [[later]]\n```\n[[pr]]\n```';
    const out = wikiLinksToMarkdown(body, [link('[[pr]]', 'pr', { kind: 'skill', id: 'pr', origin: 'project' })]);
    assert.equal(out, '`[[pr]]` and [pr](/policy/skills?name=pr) and [[later]]\n```\n[[pr]]\n```');
    assert.equal(wikiLinksToMarkdown(body, []), body);
  });
});
