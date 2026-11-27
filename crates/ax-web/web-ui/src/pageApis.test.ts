import assert from 'node:assert/strict';
import { afterEach, test } from 'node:test';
import {
  approveSpec,
  createAlternativePlan,
  fetchPlanRevisions,
  fetchSpecList,
  fetchSpecRevisions,
  saveSpecPlanRevision,
  saveSpecRevision,
} from './specsApi.ts';
import { cancelLspEnrich, fetchLspProgress, fetchLspStatus, installLspServer, runLspEnrich } from './api.ts';
import { fetchCatalogAudits } from './policyApi.ts';

const realFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = realFetch;
});

const calls: { url: string; method: string; body: string | null }[] = [];

function reply(status: number, body: string, contentType = 'application/json') {
  calls.length = 0;
  globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    calls.push({ url: String(input), method: init?.method ?? 'GET', body: (init?.body as string | undefined) ?? null });
    return new Response(body === '' ? null : body, { status, headers: { 'Content-Type': contentType } });
  }) as typeof fetch;
}

const HTML_502 = '<html><body>Bad Gateway</body></html>';

async function rejectsWithStatus(p: Promise<unknown>, status: number, message: RegExp) {
  await assert.rejects(p, (e: unknown) => {
    assert.ok(e instanceof Error, 'an Error');
    assert.equal(e.name, 'ApiError');
    assert.equal((e as Error & { status?: number }).status, status);
    assert.match(e.message, message);
    return true;
  });
}

test('every spec call turns an HTML proxy error into an API error, not a JSON parse error', async () => {
  reply(502, HTML_502, 'text/html');
  await rejectsWithStatus(fetchSpecList(), 502, /HTTP 502/);
  await rejectsWithStatus(fetchSpecRevisions('a'), 502, /HTTP 502/);
  await rejectsWithStatus(fetchPlanRevisions('p'), 502, /HTTP 502/);
  await rejectsWithStatus(approveSpec('a', 2), 502, /HTTP 502/);
  await rejectsWithStatus(saveSpecRevision('a', 1, 'body'), 502, /HTTP 502/);
  await rejectsWithStatus(saveSpecPlanRevision({ planId: 'p', basedOn: 1, body: 'b', sessionId: 's', turnId: 't' }), 502, /HTTP 502/);
  await rejectsWithStatus(
    createAlternativePlan({ projectId: 'x', specId: 'a', planId: 'p', planTitle: 'T', planBody: 'B', sessionId: 's', turnId: 't' }),
    502,
    /HTTP 502/,
  );
});

test('an expired share token on a spec call reports the server message and status', async () => {
  reply(401, JSON.stringify({ error: 'token expired' }));
  await rejectsWithStatus(fetchSpecRevisions('a'), 401, /token expired/);
});

test('spec calls keep their URLs and bodies', async () => {
  reply(200, JSON.stringify({ revisions: [] }));
  await fetchSpecRevisions('a/b');
  assert.equal(calls[0].url, '/api/specs/a%2Fb/revisions');
  await fetchPlanRevisions('p 1');
  assert.equal(calls[1].url, '/api/specs/plan/p%201/revisions');

  reply(200, JSON.stringify({ ok: true, version: 3 }));
  assert.deepEqual(await approveSpec('a', 3), { ok: true, version: 3 });
  assert.equal(calls[0].url, '/api/specs/a/approve');
  assert.equal(calls[0].method, 'POST');
  assert.deepEqual(JSON.parse(calls[0].body ?? ''), { version: 3 });

  reply(200, JSON.stringify({ ok: true, version: 4 }));
  await saveSpecRevision('a', 3, 'text');
  assert.equal(calls[0].url, '/api/specs/a/revisions');
  assert.deepEqual(JSON.parse(calls[0].body ?? ''), { basedOn: 3, body: 'text' });
});

test('the LSP calls report HTTP errors instead of an empty server list', async () => {
  reply(500, HTML_502, 'text/html');
  await rejectsWithStatus(fetchLspStatus(), 500, /HTTP 500/);
  await rejectsWithStatus(fetchLspProgress(), 500, /HTTP 500/);
  await rejectsWithStatus(installLspServer('rust'), 500, /HTTP 500/);
  await rejectsWithStatus(cancelLspEnrich(), 500, /HTTP 500/);
  await rejectsWithStatus(runLspEnrich(50), 500, /HTTP 500/);

  reply(200, JSON.stringify({ servers: [{ id: 'rust', available: true }] }));
  assert.deepEqual(await fetchLspStatus(), { servers: [{ id: 'rust', available: true }] });
  assert.equal(calls[0].url, '/api/lsp/status');

  reply(200, JSON.stringify({ ok: true, report: {} }));
  await runLspEnrich(50);
  assert.equal(calls[0].url, '/api/lsp/enrich');
  assert.deepEqual(JSON.parse(calls[0].body ?? ''), { limit: 50 });
});

test('catalog audits reject on an HTTP error and keep the rows on success', async () => {
  reply(502, HTML_502, 'text/html');
  await assert.rejects(fetchCatalogAudits('rules'), /HTTP 502/);
  reply(200, JSON.stringify([{ name: 'r1', verdict: 'pass', mechanical_warns: 0, mechanical_fails: 0 }]));
  const rows = await fetchCatalogAudits('skills');
  assert.equal(rows[0].name, 'r1');
  assert.equal(calls[0].url, '/api/policy/skills/catalog/audits');
});
