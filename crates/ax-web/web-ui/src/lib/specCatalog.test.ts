import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { createSpecCatalog, type SpecLink } from './specCatalog.ts';

function response(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
}

const plan: SpecLink = { id: 'spec-1', title: 'Plan', sessions: 'abc' };

test('a failed fetch is not cached', async () => {
  let calls = 0;
  const catalog = createSpecCatalog(async () => {
    calls++;
    if (calls === 1) throw new Error('offline');
    return response(200, { specs: [plan] });
  });
  assert.deepEqual(await catalog.load(), []);
  assert.deepEqual(await catalog.load(), [plan]);
  assert.equal(calls, 2);
});

test('a non-OK answer is not cached', async () => {
  let calls = 0;
  const catalog = createSpecCatalog(async () => {
    calls++;
    return calls === 1 ? response(500, { error: 'boom', specs: [plan] }) : response(200, { specs: [plan] });
  });
  assert.deepEqual(await catalog.load(), []);
  assert.deepEqual(await catalog.load(), [plan]);
});

test('a good answer is reused until it expires or is invalidated', async () => {
  let calls = 0;
  let clock = 0;
  const catalog = createSpecCatalog(async () => { calls++; return response(200, { specs: [plan] }); }, () => clock, 1000);
  await catalog.load();
  await catalog.load();
  assert.equal(calls, 1);
  clock = 1001;
  await catalog.load();
  assert.equal(calls, 2);
  catalog.invalidate();
  await catalog.load();
  assert.equal(calls, 3);
});

test('a slow answer from before an invalidation does not replace the new cache', async () => {
  let release: (value: Response) => void = () => {};
  let calls = 0;
  const catalog = createSpecCatalog(() => {
    calls++;
    if (calls === 1) return new Promise<Response>((resolve) => { release = resolve; });
    return Promise.resolve(response(200, { specs: [] }));
  });
  const stale = catalog.load();
  catalog.invalidate();
  assert.deepEqual(await catalog.load(), []);
  release(response(500, {}));
  await stale;
  assert.deepEqual(await catalog.load(), []);
  assert.equal(calls, 2);
});

test('SessionHop uses the expiring catalog and drops it on a project switch', () => {
  const source = readFileSync(new URL('../components/SessionHop.tsx', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /let specCatalog/);
  assert.match(source, /WORKSPACE_SWITCHED/);
  assert.match(source, /specCatalog\.invalidate\(\)/);
});
