import assert from 'node:assert/strict';
import test from 'node:test';

import { streamGraph, type GraphStreamHandlers } from './api.ts';

type FetchImpl = (url: string, init?: RequestInit) => Promise<Response>;

function withFetch(impl: FetchImpl): () => void {
  const original = globalThis.fetch;
  globalThis.fetch = impl as typeof fetch;
  return () => {
    globalThis.fetch = original;
  };
}

function sse(chunks: string[], failAfter = false): Response {
  const enc = new TextEncoder();
  let i = 0;
  const body = new ReadableStream<Uint8Array>({
    pull(ctrl) {
      if (i < chunks.length) ctrl.enqueue(enc.encode(chunks[i++]));
      else if (failAfter) ctrl.error(new TypeError('network connection lost'));
      else ctrl.close();
    },
  });
  return new Response(body, { status: 200, headers: { 'Content-Type': 'text/event-stream' } });
}

function recorder() {
  const calls: string[] = [];
  let errors: unknown[] = [];
  let settle: () => void = () => {};
  const settled = new Promise<void>((resolve) => {
    settle = resolve;
  });
  const handlers: GraphStreamHandlers = {
    onMeta: () => calls.push('meta'),
    onNodes: (n) => calls.push(`nodes:${n.length}`),
    onEdges: (e) => calls.push(`edges:${e.length}`),
    onDone: () => {
      calls.push('done');
      settle();
    },
    onError: (e) => {
      errors = [...errors, e];
      calls.push('error');
      settle();
    },
  };
  return { calls, handlers, settled, errors: () => errors };
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 20));

test('a failed stream and failed fallback reach onError, not an unhandled rejection', { timeout: 3000 }, async () => {
  const unhandled: unknown[] = [];
  const onUnhandled = (e: unknown) => unhandled.push(e);
  process.on('unhandledRejection', onUnhandled);
  const restore = withFetch(async () => new Response('{"error":"boom"}', { status: 500 }));
  try {
    const r = recorder();
    streamGraph({}, r.handlers);
    await r.settled;
    await tick();
    assert.deepEqual(r.calls, ['error']);
    assert.match(String(r.errors()[0]), /boom/);
    assert.deepEqual(unhandled, []);
  } finally {
    restore();
    process.off('unhandledRejection', onUnhandled);
  }
});

test('a connection that drops mid-stream reaches onError', { timeout: 3000 }, async () => {
  const restore = withFetch(async () =>
    sse(['data: {"type":"meta","total_nodes":1,"truncated":false,"node_count":1,"edge_count":0}\n\n'], true),
  );
  try {
    const r = recorder();
    streamGraph({}, r.handlers);
    await r.settled;
    assert.deepEqual(r.calls, ['meta', 'error']);
  } finally {
    restore();
  }
});

test('a stream that closes without a done event still calls onDone', { timeout: 3000 }, async () => {
  const restore = withFetch(async () => sse(['data: {"type":"nodes","nodes":[]}\n\n']));
  try {
    const r = recorder();
    streamGraph({}, r.handlers);
    await r.settled;
    assert.deepEqual(r.calls, ['nodes:0', 'done']);
  } finally {
    restore();
  }
});

test('done is reported once when the server sends it', { timeout: 3000 }, async () => {
  const restore = withFetch(async () => sse(['data: {"type":"done"}\n\n']));
  try {
    const r = recorder();
    streamGraph({}, r.handlers);
    await r.settled;
    await tick();
    assert.deepEqual(r.calls, ['done']);
  } finally {
    restore();
  }
});

test('the fallback fetch is cancelled with the stream and its result is ignored', { timeout: 3000 }, async () => {
  let fallbackSignal: AbortSignal | undefined;
  let release: () => void = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const restore = withFetch(async (url, init) => {
    if (url.includes('/graph/stream')) return new Response('', { status: 404 });
    fallbackSignal = init?.signal ?? undefined;
    await gate;
    return new Response(JSON.stringify({ nodes: [{ id: 'a' }], edges: [], total_nodes: 1, truncated: false }), {
      status: 200,
    });
  });
  try {
    const r = recorder();
    const abort = streamGraph({}, r.handlers);
    await tick();
    assert.ok(fallbackSignal, 'fallback fetch must receive the abort signal');
    abort();
    assert.equal(fallbackSignal?.aborted, true);
    release();
    await tick();
    assert.deepEqual(r.calls, []);
  } finally {
    restore();
  }
});
