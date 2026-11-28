import assert from 'node:assert/strict';
import { beforeEach, test } from 'node:test';

import { accessJson, clearAccessCsrf } from './accessApi.ts';

type Call = { url: string; headers: Record<string, string> };

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

function stubFetch(handler: (url: string, calls: Call[]) => Response): Call[] {
  const calls: Call[] = [];
  globalThis.fetch = (async (input: string, init?: RequestInit) => {
    const headers = (init?.headers ?? {}) as Record<string, string>;
    calls.push({ url: input, headers });
    return handler(input, calls);
  }) as typeof fetch;
  return calls;
}

beforeEach(() => clearAccessCsrf());

test('a write is refused locally when the session has no CSRF token', async () => {
  const calls = stubFetch(url => (url === '/api/access/session' ? json({}) : json({ ok: true })));
  await assert.rejects(
    accessJson('/api/access/team/m1', { method: 'PATCH', body: '{}' }),
    /local owner session is required/,
  );
  assert.deepEqual(calls.map(c => c.url), ['/api/access/session']);
});

test('a session body that is not JSON gives the owner-session error, not a SyntaxError', async () => {
  stubFetch(url => (url === '/api/access/session' ? new Response('<html>', { status: 200 }) : json({})));
  await assert.rejects(
    accessJson('/api/access/team/m1', { method: 'PATCH', body: '{}' }),
    /local owner session is required/,
  );
});

test('the CSRF token is fetched once and reused for later writes', async () => {
  const calls = stubFetch(url => (url === '/api/access/session' ? json({ csrf: 'tok' }) : json({ ok: true })));
  await accessJson('/api/access/a', { method: 'POST', body: '{}' });
  await accessJson('/api/access/b', { method: 'POST', body: '{}' });
  assert.deepEqual(calls.map(c => c.url), ['/api/access/session', '/api/access/a', '/api/access/b']);
  assert.equal(calls[1].headers['x-ax-access-csrf'], 'tok');
  assert.equal(calls[2].headers['x-ax-access-csrf'], 'tok');
});

test('caller headers cannot override the CSRF header', async () => {
  const calls = stubFetch(url => (url === '/api/access/session' ? json({ csrf: 'tok' }) : json({ ok: true })));
  await accessJson('/api/access/a', { method: 'POST', body: '{}', headers: { 'x-ax-access-csrf': 'other' } });
  assert.equal(calls[1].headers['x-ax-access-csrf'], 'tok');
});

test('a 403 with a cached token refetches the token once and retries', async () => {
  let token = 'old';
  const calls = stubFetch((url, seen) => {
    if (url === '/api/access/session') return json({ csrf: token });
    const sent = seen[seen.length - 1].headers['x-ax-access-csrf'];
    return sent === 'new' ? json({ ok: true }) : json({ error: 'Missing CSRF token' }, 403);
  });
  await accessJson('/api/access/a', { method: 'POST', body: '{}' }).catch(() => undefined);
  token = 'new';
  const result = await accessJson<{ ok: boolean }>('/api/access/a', { method: 'POST', body: '{}' });
  assert.deepEqual(result, { ok: true });
  assert.deepEqual(
    calls.map(c => c.url),
    ['/api/access/session', '/api/access/a', '/api/access/session', '/api/access/a', '/api/access/session', '/api/access/a'],
  );
});

test('reads do not fetch a CSRF token', async () => {
  const calls = stubFetch(() => json({ members: [] }));
  await accessJson('/api/access/team');
  assert.deepEqual(calls.map(c => c.url), ['/api/access/team']);
  assert.equal(calls[0].headers['x-ax-access-csrf'], undefined);
});
