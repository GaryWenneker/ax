import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { isChatSession, linkedSessions, specForSession } from './specSession.ts';

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

test('specs, memory, and logging link the same session in both directions', () => {
  const specsPage = readFileSync(new URL('../pages/Specs.tsx', import.meta.url), 'utf8');
  const memory = readFileSync(new URL('../pages/Memory.tsx', import.meta.url), 'utf8');
  const logging = readFileSync(new URL('../components/McpTraceLive.tsx', import.meta.url), 'utf8');
  const routes = readFileSync(new URL('./routes.ts', import.meta.url), 'utf8');
  const nav = readFileSync(new URL('./nav.ts', import.meta.url), 'utf8');
  assert.match(specsPage, /navigateRoute\(\{ page: 'memory', session: id \}\)/);
  assert.doesNotMatch(specsPage, /navigateRoute\(\{ page: 'memory', recallQuery: id \}\)/);
  assert.match(specsPage, /linkedSessions\(spec\.sessions\)/);
  assert.match(specsPage, /searchParams\.get\('spec'\)/);
  assert.match(memory, /SessionHop/);
  assert.match(memory, /fetchMemories\(\{ limit: MEMORY_PAGE, session: session \|\| undefined \}\)/);
  assert.match(memory, /if \(session\) setQ\(''\)/);
  assert.match(memory, /typed !== sessionFilter/);
  assert.match(logging, /mcp-trace-heading[\s\S]{0,240}SessionHop session=\{sessionFilter\} here="logging"/);
  assert.match(routes, /page === 'memory' && state\.session/);
  assert.match(routes, /page === 'specs' && state\.specId/);
  assert.match(
    nav,
    /id: 'policy-skills', label: 'Skills'[\s\S]*?id: 'specs', label: 'Specs & Plans'[\s\S]*?id: 'memory', label: 'Memory'/,
  );
});
