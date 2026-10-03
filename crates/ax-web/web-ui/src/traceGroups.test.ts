import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cacheGroupKey, groupHue, traceGroupLinks } from './traceGroups.ts';

const context = (group: string) =>
  `cache group=${group} lane=context enabled=1 threshold=3000 rows=1 stored_tokens=10 expired=0`;
const token = (group: string) =>
  `cache group=${group} lane=token entries=1 capacity=8192 hits=1 misses=1 evictions=0 tokenizer=1`;

test('a cache status line yields its group and other lines do not', () => {
  assert.equal(cacheGroupKey(context('sess-1')), 'sess-1');
  assert.equal(cacheGroupKey('inbound tool=ax_explore args={}'), null);
  assert.equal(cacheGroupKey('cache group=sess-1 lane=other rows=1'), null);
  assert.equal(cacheGroupKey('note group=sess-1 lane=context'), null);
});

test('one cache line is not linked', () => {
  assert.deepEqual(traceGroupLinks([context('sess-1'), 'inbound tool=ax_node']), [null, null]);
});

test('the two lines of one status call share a hue and join', () => {
  const links = traceGroupLinks([context('sess-1'), token('sess-1')]);
  assert.equal(links[0]?.hue, links[1]?.hue);
  assert.equal(links[0]?.joinNext, true);
  assert.equal(links[1]?.joinPrev, true);
  assert.equal(links[0]?.lane, 'context');
  assert.equal(links[1]?.lane, 'token');
  assert.deepEqual(
    links.map((l) => l && [l.position, l.count]),
    [[2, 2], [1, 2]],
  );
});

test('a different group keeps its own hue and does not join across', () => {
  const links = traceGroupLinks([
    context('aaaa'),
    context('bbbb'),
    token('bbbb'),
    token('aaaa'),
  ]);
  assert.equal(links[0]?.hue, links[3]?.hue);
  assert.notEqual(links[0]?.hue, links[1]?.hue);
  assert.equal(links[0]?.joinNext, false);
  assert.equal(links[1]?.joinNext, true);
  assert.equal(links[2]?.joinPrev, true);
  assert.equal(links[0]?.count, 2);
});

test('hue is a stable integer in 0..359', () => {
  const hue = groupHue('ff3a3fc2-a82b');
  assert.equal(groupHue('ff3a3fc2-a82b'), hue);
  assert.ok(Number.isInteger(hue) && hue >= 0 && hue < 360, String(hue));
});
