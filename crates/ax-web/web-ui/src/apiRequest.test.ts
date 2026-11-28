import assert from 'node:assert/strict';
import { afterEach, test } from 'node:test';
import { fetchAgentStatus, saveAgentConfig, setActiveProfile, updateAgentProfile, type AgentsConfig } from './agentApi.ts';
import { deleteMemory, fetchContextBudget, saveContextBudget, setMemoryEnabled, updateMemory } from './api.ts';
import { runMcpAudit } from './lib/mcpQuality.ts';

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
const config = { enabled_targets: [], terminal_mode: 'x', active_profile: {}, profiles: {} } as AgentsConfig;

/** The rejection is a typed API error carrying the HTTP status, never a JSON SyntaxError. */
async function rejectsWith(p: Promise<unknown>, status: number, message: RegExp) {
  await assert.rejects(p, (e: unknown) => {
    assert.ok(e instanceof Error, 'an Error');
    assert.equal(e.name, 'ApiError');
    assert.equal((e as Error & { status?: number }).status, status);
    assert.match(e.message, message);
    return true;
  });
}

test('a JSON 500 from the agent status endpoint is an error, not a status object', async () => {
  reply(500, JSON.stringify({ error: 'status failed' }));
  await rejectsWith(fetchAgentStatus(), 500, /status failed/);
});

test('an HTML 502 from a proxy is an API error for every agent helper', async () => {
  reply(502, HTML_502, 'text/html');
  await rejectsWith(saveAgentConfig(config), 502, /HTTP 502/);
  await rejectsWith(setActiveProfile('claude', 'work'), 502, /HTTP 502/);
  await rejectsWith(updateAgentProfile('claude', 'work', { label: 'Work' }), 502, /HTTP 502/);
});

test('an HTML page with status 200, such as a login page, is an API error', async () => {
  reply(200, HTML_502, 'text/html');
  await rejectsWith(fetchAgentStatus(), 200, /not JSON/);
  await rejectsWith(setMemoryEnabled('m1', true), 200, /not JSON/);
});

test('memory writes report the status and server message', async () => {
  reply(409, JSON.stringify({ error: 'memory changed' }));
  await rejectsWith(updateMemory('m1', { title: 't', body: 'b' }), 409, /memory changed/);
  reply(502, HTML_502, 'text/html');
  await rejectsWith(deleteMemory('m1'), 502, /HTTP 502/);
});

test('an MCP audit behind a broken proxy is an API error', async () => {
  reply(502, HTML_502, 'text/html');
  await rejectsWith(runMcpAudit(), 502, /HTTP 502/);
});

test('successful helpers send the same requests and return the parsed body', async () => {
  reply(200, JSON.stringify({ ok: true, targets: [], catalog: [{ id: 'claude' }], config, all_targets: [] }));
  const status = await fetchAgentStatus();
  assert.deepEqual(status.targets, [{ id: 'claude' }]);
  assert.equal(calls[0].url, '/api/agent/status');

  reply(200, JSON.stringify({ ok: true }));
  assert.deepEqual(await saveAgentConfig(config), { ok: true });
  assert.equal(calls[0].method, 'PUT');
  assert.equal(calls[0].url, '/api/agent/config');
  assert.deepEqual(JSON.parse(calls[0].body ?? ''), { config });

  reply(200, JSON.stringify({ ok: true }));
  await setActiveProfile('claude', 'work');
  assert.equal(calls[0].url, '/api/agent/profiles/active');
  assert.deepEqual(JSON.parse(calls[0].body ?? ''), { agent: 'claude', profile_id: 'work' });

  reply(200, JSON.stringify({ ok: true }));
  assert.deepEqual(await setMemoryEnabled('a/b', false), { ok: true });
  assert.equal(calls[0].url, '/api/memory/a%2Fb/enabled');
  assert.equal(calls[0].method, 'PATCH');

  reply(204, '');
  await deleteMemory('m1');
  assert.equal(calls[0].method, 'DELETE');
});

test('the context budget read and write report HTTP errors instead of parsing HTML', async () => {
  reply(502, HTML_502, 'text/html');
  await rejectsWith(fetchContextBudget(), 502, /HTTP 502/);
  await rejectsWith(saveContextBudget(9000), 502, /HTTP 502/);

  reply(200, JSON.stringify({ tokens: 12000 }));
  assert.deepEqual(await fetchContextBudget(), { tokens: 12000 });
  assert.equal(calls[0].url, '/api/context-budget/');

  reply(200, JSON.stringify({ ok: true, tokens: 9000 }));
  await saveContextBudget(9000);
  assert.equal(calls[0].method, 'PUT');
  assert.deepEqual(JSON.parse(calls[0].body ?? ''), { tokens: 9000 });
});
