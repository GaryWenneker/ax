import assert from 'node:assert/strict';
import { afterEach, test } from 'node:test';
import { streamAgentChat, type AgentStreamEvent } from './agentApi.ts';
import { streamGraph, type GraphNode } from './api.ts';

const realFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = realFetch;
});

/** Serve `chunks` as one SSE response body, split exactly where given. */
function serve(chunks: (string | Uint8Array)[]) {
  globalThis.fetch = (async () => {
    const enc = new TextEncoder();
    const body = new ReadableStream<Uint8Array>({
      start(c) {
        for (const chunk of chunks) c.enqueue(typeof chunk === 'string' ? enc.encode(chunk) : chunk);
        c.close();
      },
    });
    return new Response(body, { status: 200, headers: { 'Content-Type': 'text/event-stream' } });
  }) as typeof fetch;
}

async function chat(chunks: (string | Uint8Array)[]): Promise<AgentStreamEvent[]> {
  serve(chunks);
  const events: AgentStreamEvent[] = [];
  await streamAgentChat('hi', {}, (ev) => events.push(ev));
  return events;
}

test('a final event without a trailing blank line is delivered', async () => {
  const events = await chat(['data: {"type":"token","text":"a"}\n\n', 'data: {"type":"done"}\n']);
  assert.deepEqual(events, [{ type: 'token', text: 'a' }, { type: 'done' }]);
});

test('a final event with no newline at all is delivered', async () => {
  const events = await chat(['data: {"type":"done","ok":true}']);
  assert.deepEqual(events, [{ type: 'done', ok: true }]);
});

test('CRLF line endings are accepted', async () => {
  const events = await chat(['data: {"type":"token","text":"a"}\r\n\r\ndata: {"type":"done"}\r\n\r\n']);
  assert.deepEqual(events, [{ type: 'token', text: 'a' }, { type: 'done' }]);
});

test('a data field without a space after the colon is accepted', async () => {
  const events = await chat(['data:{"type":"done"}\n\n']);
  assert.deepEqual(events, [{ type: 'done' }]);
});

test('a multi-byte character split across chunks and the stream end is decoded', async () => {
  const bytes = new TextEncoder().encode('data: {"type":"token","text":"é"}');
  const cut = bytes.length - 3; // inside the two-byte é
  const events = await chat([bytes.slice(0, cut), bytes.slice(cut)]);
  assert.deepEqual(events, [{ type: 'token', text: 'é' }]);
});

test('comments and malformed data lines are skipped', async () => {
  const events = await chat([': keep-alive\n\ndata: {not json}\n\ndata: {"type":"done"}\n\n']);
  assert.deepEqual(events, [{ type: 'done' }]);
});

function graph(chunks: string[]): Promise<{ nodes: GraphNode[]; done: boolean; error?: Error }> {
  serve(chunks);
  return new Promise((resolve) => {
    const nodes: GraphNode[] = [];
    streamGraph({}, {
      onNodes: (n) => nodes.push(...n),
      onDone: () => resolve({ nodes, done: true }),
      onError: (error) => resolve({ nodes, done: false, error }),
    });
  });
}

test('the graph stream delivers a last batch that has no trailing blank line', async () => {
  const result = await graph(['data: {"type":"nodes","nodes":[{"id":"a"}]}\n\n', 'data: {"type":"nodes","nodes":[{"id":"b"}]}']);
  assert.equal(result.done, true);
  assert.deepEqual(result.nodes.map((n) => n.id), ['a', 'b']);
});

test('the graph stream accepts CRLF framing', async () => {
  const result = await graph(['data: {"type":"nodes","nodes":[{"id":"a"}]}\r\n\r\ndata: {"type":"done"}\r\n\r\n']);
  assert.equal(result.done, true);
  assert.deepEqual(result.nodes.map((n) => n.id), ['a']);
});
