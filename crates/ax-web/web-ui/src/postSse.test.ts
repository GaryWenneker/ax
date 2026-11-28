import assert from 'node:assert/strict';
import { afterEach, test } from 'node:test';
import {
  streamSonarInstall,
  streamSonarScanProject,
  streamSonarStart,
  streamSonarStop,
  type SonarStreamEvent,
} from './shipApi.ts';
import { streamWorkspaceInit, type WorkspaceStreamEvent } from './workspaceApi.ts';

const realFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = realFetch;
});

const calls: { url: string; method?: string; body?: string; contentType: string | null }[] = [];

function serve(status: number, chunks: string[], contentType = 'text/event-stream') {
  calls.length = 0;
  globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    calls.push({
      url: String(input),
      method: init?.method,
      body: init?.body as string | undefined,
      contentType: new Headers(init?.headers).get('Content-Type'),
    });
    const enc = new TextEncoder();
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        for (const c of chunks) controller.enqueue(enc.encode(c));
        controller.close();
      },
    });
    return new Response(body, { status, headers: { 'Content-Type': contentType } });
  }) as typeof fetch;
}

const SONAR: [string, (on: (ev: SonarStreamEvent) => void) => Promise<void>, string, string][] = [
  ['install', (on) => streamSonarInstall(on), '/api/ship/sonar/install/stream', '{}'],
  ['start', (on) => streamSonarStart(on), '/api/ship/sonar/start/stream', '{}'],
  ['stop', (on) => streamSonarStop(on), '/api/ship/sonar/stop/stream', '{}'],
  ['scan', (on) => streamSonarScanProject('org.k', on), '/api/ship/sonar/scan/stream', '{"project_key":"org.k"}'],
];

for (const [name, run, url, body] of SONAR) {
  test(`sonar ${name} posts JSON and delivers a final done event without a trailing blank line`, async () => {
    serve(200, ['data: {"type":"log","line":"a"}\r\n\r\n', 'data:{"type":"done","ok":true}']);
    const events: SonarStreamEvent[] = [];
    await run((ev) => events.push(ev));
    assert.deepEqual(events, [{ type: 'log', line: 'a' }, { type: 'done', ok: true }]);
    assert.deepEqual(calls, [{ url, method: 'POST', body, contentType: 'application/json' }]);
  });

  test(`sonar ${name} rejects an HTTP error with an API error carrying the status and server message`, async () => {
    serve(409, [JSON.stringify({ error: 'SonarQube is busy' })], 'application/json');
    await assert.rejects(run(() => {}), (e: unknown) => {
      assert.ok(e instanceof Error);
      assert.equal(e.name, 'ApiError');
      assert.equal((e as Error & { status?: number }).status, 409);
      assert.equal(e.message, 'SonarQube is busy');
      return true;
    });
  });
}

test('workspace init reports the server message of an HTTP error in its done event', async () => {
  serve(400, [JSON.stringify({ error: 'not a directory' })], 'application/json');
  const events: WorkspaceStreamEvent[] = [];
  await streamWorkspaceInit('/nope', (ev) => events.push(ev));
  assert.deepEqual(events, [{ type: 'done', ok: false, error: 'not a directory' }]);
  assert.deepEqual(calls, [{ url: '/api/workspace/init/stream', method: 'POST', body: '{"path":"/nope"}', contentType: 'application/json' }]);
});

test('workspace init reports a bare HTTP status for an error page', async () => {
  serve(502, ['<html>Bad Gateway</html>'], 'text/html');
  const events: WorkspaceStreamEvent[] = [];
  await streamWorkspaceInit('/w', (ev) => events.push(ev));
  assert.deepEqual(events, [{ type: 'done', ok: false, error: 'HTTP 502' }]);
});
