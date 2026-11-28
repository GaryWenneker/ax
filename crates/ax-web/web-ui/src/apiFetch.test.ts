import assert from 'node:assert/strict';
import { afterEach, test } from 'node:test';
import { apiFetch } from './api.ts';
import { deletePolicyRule, fetchCatalogAudits, savePolicyRule } from './policyApi.ts';
import { fetchShareConfig, signOutMicrosoft } from './shareApi.ts';
import { fetchShipStatus, saveShipConfig } from './shipApi.ts';

const realFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = realFetch;
});

const calls: { url: string; init: RequestInit | undefined }[] = [];

function reply(status: number, body: string, contentType = 'application/json') {
  calls.length = 0;
  globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    calls.push({ url: String(input), init });
    return new Response(body === '' ? null : body, { status, headers: { 'Content-Type': contentType } });
  }) as typeof fetch;
}

const LOGIN_PAGE = '<html><body>Sign in</body></html>';

async function rejectsWith(p: Promise<unknown>, status: number, message: RegExp) {
  await assert.rejects(p, (e: unknown) => {
    assert.ok(e instanceof Error, 'an Error');
    assert.equal(e.name, 'ApiError');
    assert.equal((e as Error & { status?: number }).status, status);
    assert.match(e.message, message);
    return true;
  });
}

test('a 200 login page is an API error for ship, policy and share reads, not an empty object', async () => {
  reply(200, LOGIN_PAGE, 'text/html');
  await rejectsWith(fetchShipStatus(), 200, /not JSON/);
  await rejectsWith(fetchCatalogAudits('rules'), 200, /not JSON/);
  await rejectsWith(fetchShareConfig(), 200, /not JSON/);
});

test('a 204 reply resolves without a body for every area', async () => {
  reply(204, '');
  assert.equal(await signOutMicrosoft(), undefined);
  assert.equal(await deletePolicyRule('r1'), undefined);
});

test('error replies carry the status, the server message and any field errors', async () => {
  reply(422, JSON.stringify({ error: 'invalid rule', fields: { name: 'required', priority: 'too high' } }));
  await rejectsWith(
    savePolicyRule({} as Parameters<typeof savePolicyRule>[0]),
    422,
    /^invalid rule \(name: required; priority: too high\)$/,
  );
  reply(500, JSON.stringify({ error: 'disk full' }));
  await rejectsWith(saveShipConfig({} as Parameters<typeof saveShipConfig>[0]), 500, /^disk full$/);
  reply(502, LOGIN_PAGE, 'text/html');
  await rejectsWith(fetchShareConfig(), 502, /^HTTP 502$/);
});

test('caller headers extend the JSON content type instead of replacing it', async () => {
  reply(200, '{"ok":true}');
  await apiFetch('/api/x', { method: 'POST', body: '{}', headers: { 'X-Trace': 't1' } });
  const headers = new Headers(calls[0].init?.headers);
  assert.equal(headers.get('Content-Type'), 'application/json');
  assert.equal(headers.get('X-Trace'), 't1');
  assert.equal(calls[0].init?.method, 'POST');
});

test('a caller content type wins over the default', async () => {
  reply(200, '{"ok":true}');
  await apiFetch('/api/x', { method: 'POST', body: 'a,b', headers: { 'Content-Type': 'text/csv' } });
  assert.equal(new Headers(calls[0].init?.headers).get('Content-Type'), 'text/csv');
});

test('a successful JSON reply is returned as data', async () => {
  reply(200, JSON.stringify({ branch: 'main', report: null, config: { ship: {} } }));
  const status = await fetchShipStatus();
  assert.equal(status.branch, 'main');
  assert.equal(calls[0].url, '/api/ship/status');
});

test('vault mount and folder calls reject a 200 reply that is not JSON instead of returning null', async () => {
  const { fetchMountStatus, fetchVaultFolders } = await import('./vaultMount.ts');
  reply(200, LOGIN_PAGE, 'text/html');
  await assert.rejects(fetchMountStatus(), (e: unknown) => {
    assert.match((e as { message: string }).message, /not JSON/);
    assert.equal((e as { command: unknown }).command, null);
    return true;
  });
  await assert.rejects(fetchVaultFolders(), /not JSON/);
});

test('vault mount errors keep the server message and command', async () => {
  const { fetchMountStatus } = await import('./vaultMount.ts');
  reply(500, JSON.stringify({ error: 'mount failed', command: 'mount -t webdav' }));
  await assert.rejects(fetchMountStatus(), (e: unknown) => {
    assert.ok(e instanceof Error);
    assert.equal(e.message, 'mount failed');
    assert.equal((e as Error & { command: string | null }).command, 'mount -t webdav');
    return true;
  });
});
