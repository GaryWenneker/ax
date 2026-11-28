import assert from 'node:assert/strict';
import test from 'node:test';

import { checkSession, nextAuthState, retryDelayMs, type AuthState } from './authSession.ts';

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

function fetchWith(routes: Record<string, () => Response | Promise<Response>>) {
  return (async (url: string) => {
    const route = routes[url];
    if (!route) throw new Error(`unexpected ${url}`);
    return route();
  }) as typeof fetch;
}

const open: AuthState = { mode: 'open', notice: null, failures: 0 };
const loading: AuthState = { mode: 'loading', notice: null, failures: 0 };

test('auth disabled means open', async () => {
  const outcome = await checkSession(fetchWith({ '/api/web-auth/status': () => json({ enabled: false }) }));
  assert.equal(outcome.kind, 'open');
});

test('a 401 session means login', async () => {
  const outcome = await checkSession(
    fetchWith({
      '/api/web-auth/status': () => json({ enabled: true }),
      '/api/web-auth/session': () => json({ error: 'no session' }, 401),
    }),
  );
  assert.equal(outcome.kind, 'login');
});

test('a thrown fetch, a 5xx and a non-JSON body are transient', async () => {
  const down = await checkSession(
    fetchWith({
      '/api/web-auth/status': () => {
        throw new TypeError('Failed to fetch');
      },
    }),
  );
  assert.equal(down.kind, 'unavailable');
  const busy = await checkSession(
    fetchWith({
      '/api/web-auth/status': () => json({ enabled: true }),
      '/api/web-auth/session': () => json({}, 503),
    }),
  );
  assert.equal(busy.kind, 'unavailable');
  const proxy = await checkSession(
    fetchWith({ '/api/web-auth/status': () => new Response('<html>Bad gateway</html>', { status: 502 }) }),
  );
  assert.equal(proxy.kind, 'unavailable');
});

test('a transient failure while open keeps the app mounted and shows a notice', () => {
  const next = nextAuthState(open, { kind: 'unavailable', message: 'Failed to fetch' });
  assert.equal(next.mode, 'open');
  assert.match(next.notice ?? '', /Failed to fetch/);
  assert.equal(next.failures, 1);
});

test('only a definite 401 while open switches to login', () => {
  assert.equal(nextAuthState(open, { kind: 'login' }).mode, 'login');
});

test('recovery clears the notice and the failure count', () => {
  const degraded = nextAuthState(open, { kind: 'unavailable', message: 'x' });
  const back = nextAuthState(degraded, { kind: 'open', origin: null });
  assert.deepEqual(back, { mode: 'open', notice: null, failures: 0 });
});

test('a failure before the app ever opened shows the error page', () => {
  const next = nextAuthState(loading, { kind: 'unavailable', message: 'down' });
  assert.equal(next.mode, 'error');
  assert.equal(next.notice, 'down');
});

test('retries back off and are capped', () => {
  assert.equal(retryDelayMs(1), 2_000);
  assert.equal(retryDelayMs(2), 4_000);
  assert.equal(retryDelayMs(3), 8_000);
  assert.equal(retryDelayMs(50), 30_000);
});
