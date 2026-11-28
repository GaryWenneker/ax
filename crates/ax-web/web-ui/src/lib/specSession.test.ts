import assert from 'node:assert/strict';
import test from 'node:test';
import { NAV_SECTIONS } from './nav.ts';
import { buildPath, parseLocation } from './routes.ts';
import { isChatSession, linkedSessions, sessionHopTargets, specForSession } from './specSession.ts';

const specs = [
  { id: 'older', title: 'Older', sessions: 'plan-older' },
  { id: 'live', title: 'Link plans', sessions: 'plan-live 2057ea92-20ba-4bce-a732-3cbf46e61b9e' },
];

test('a real session is the link when a plan slug is also bound', () => {
  assert.deepEqual(linkedSessions('plan-live 2057ea92-20ba-4bce-a732-3cbf46e61b9e'), [
    '2057ea92-20ba-4bce-a732-3cbf46e61b9e',
  ]);
});

test('a plan slug stays the link when it is the only session', () => {
  assert.deepEqual(linkedSessions('plan-older'), ['plan-older']);
  assert.deepEqual(linkedSessions('  '), []);
});

test('sync and blank ids are not chat sessions', () => {
  assert.equal(isChatSession('2057ea92-20ba-4bce-a732-3cbf46e61b9e'), true);
  assert.equal(isChatSession('plan-older'), true);
  assert.equal(isChatSession('sync'), false);
  assert.equal(isChatSession('Unlinked'), false);
  assert.equal(isChatSession(''), false);
});

test('a session opens the spec that lists it', () => {
  assert.equal(specForSession(specs, '2057ea92-20ba-4bce-a732-3cbf46e61b9e')?.id, 'live');
  assert.equal(specForSession(specs, 'plan-older')?.id, 'older');
  assert.equal(specForSession(specs, 'missing'), undefined);
});

test('the session hop links Logging, Memory and the plan, without the current page', () => {
  const id = '2057ea92-20ba-4bce-a732-3cbf46e61b9e';
  assert.deepEqual(sessionHopTargets(id, 'memory', { id: 'live' }), [
    { label: 'Logging', route: { page: 'logging', session: id } },
    { label: 'Plan', route: { page: 'specs', specId: 'live' } },
  ]);
  assert.deepEqual(sessionHopTargets(id, 'logging', null), [{ label: 'Memory', route: { page: 'memory', session: id } }]);
  assert.deepEqual(sessionHopTargets('sync', 'logging', { id: 'live' }), []);
});

test('session and spec links survive the URL round trip in both directions', () => {
  const id = '2057ea92-20ba-4bce-a732-3cbf46e61b9e';
  for (const page of ['memory', 'logging'] as const) {
    const url = new URL(buildPath({ page, session: id }), 'http://ax.test');
    const back = parseLocation({ pathname: url.pathname, search: url.search } as Location);
    assert.equal(back.page, page);
    assert.equal(back.session, id);
  }
  const url = new URL(buildPath({ page: 'specs', specId: 'live' }), 'http://ax.test');
  const back = parseLocation({ pathname: url.pathname, search: url.search } as Location);
  assert.equal(back.page, 'specs');
  assert.equal(back.specId, 'live');
});

test('Specs & Plans sits between Skills and Memory in the navigation', () => {
  const ids = NAV_SECTIONS.flatMap((section) => section.items.map((item) => item.id));
  const at = (id: string) => ids.indexOf(id);
  assert.ok(at('policy-skills') >= 0 && at('policy-skills') < at('specs') && at('specs') < at('memory'), ids.join(', '));
});
