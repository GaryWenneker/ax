import assert from 'node:assert/strict';
import { afterEach, test } from 'node:test';
import { reloadMcpDaemon } from './mcpReload.ts';

const realFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = realFetch;
});

function reply(status: number, body: string) {
  globalThis.fetch = (async () => new Response(body, { status })) as typeof fetch;
}

test('an HTML 502 is reported as an HTTP error, not a JSON parse error', async () => {
  reply(502, '<html>Bad Gateway</html>');
  assert.deepEqual(await reloadMcpDaemon(), { kind: 'err', text: 'HTTP 502' });
});

test('the server error or hint is shown on failure', async () => {
  reply(500, JSON.stringify({ error: 'lock held' }));
  assert.deepEqual(await reloadMcpDaemon(), { kind: 'err', text: 'lock held' });
  reply(500, JSON.stringify({ hint: 'restart the IDE' }));
  assert.deepEqual(await reloadMcpDaemon(), { kind: 'err', text: 'restart the IDE' });
  reply(200, JSON.stringify({ ok: false, error: 'daemon refused' }));
  assert.deepEqual(await reloadMcpDaemon(), { kind: 'err', text: 'daemon refused' });
});

test('a successful reload names the new pid', async () => {
  reply(200, JSON.stringify({ ok: true, startedPid: 42, hint: 'Done.' }));
  assert.deepEqual(await reloadMcpDaemon(), { kind: 'ok', text: 'MCP daemon reloaded pid 42. Done.' });
});
