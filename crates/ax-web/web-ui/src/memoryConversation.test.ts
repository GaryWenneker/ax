import { test } from 'node:test';
import assert from 'node:assert/strict';
import { conversationKey, conversationLinks } from './memoryConversation.ts';

const turn = (key: string | null, prompt = 'Fix it') =>
  ({ body: key ? `${prompt}\n\nRequest: g1\n\nConversation: ${key}\n\nFiles: a.rs` : prompt });

test('reads the conversation key from a turn body', () => {
  assert.equal(conversationKey(turn('0a1b2c3d4e5f').body), '0a1b2c3d4e5f');
});

test('a body without the line or with a malformed key has no conversation', () => {
  assert.equal(conversationKey('Fix it\n\nRequest: g1'), null);
  assert.equal(conversationKey('Fix it\n\nConversation: xyz'), null);
  assert.equal(conversationKey('I said Conversation: 0a1b2c3d4e5f inline'), null);
});

test('a conversation with a single shown turn is not linked', () => {
  assert.deepEqual(conversationLinks([turn('aaaaaaaaaaaa'), turn(null)]), [null, null]);
});

test('adjacent turns of one conversation join, newest first gets the highest position', () => {
  const links = conversationLinks([turn('aaaaaaaaaaaa'), turn('aaaaaaaaaaaa'), turn('aaaaaaaaaaaa')]);
  assert.deepEqual(
    links.map((l) => l && [l.position, l.count, l.joinPrev, l.joinNext]),
    [
      [3, 3, false, true],
      [2, 3, true, true],
      [1, 3, true, false],
    ],
  );
});

test('an interleaved conversation keeps its color and count but does not join across others', () => {
  const links = conversationLinks([
    turn('aaaaaaaaaaaa'),
    turn('bbbbbbbbbbbb'),
    turn('aaaaaaaaaaaa'),
    turn('bbbbbbbbbbbb'),
  ]);
  const [a1, b1, a2, b2] = links;
  assert.ok(a1 && a2 && b1 && b2);
  assert.equal(a1.hue, a2.hue);
  assert.equal(b1.hue, b2.hue);
  assert.notEqual(a1.hue, b1.hue);
  assert.equal(a1.count, 2);
  assert.deepEqual([a1.joinNext, a2.joinPrev], [false, false]);
});

test('hue is a stable integer in 0..359 derived from the key', () => {
  const [l] = conversationLinks([turn('ffffffffffff'), turn('ffffffffffff')]);
  assert.ok(l);
  assert.ok(Number.isInteger(l.hue) && l.hue >= 0 && l.hue < 360, String(l.hue));
  assert.equal(conversationLinks([turn('ffffffffffff'), turn('ffffffffffff')])[0]?.hue, l.hue);
});
