import assert from 'node:assert/strict';
import test from 'node:test';
import {
  browseWorkspace,
  fetchPurgePlan,
  fetchWorkspaceCurrent,
  fetchWorkspaceRecent,
  mkdirWorkspace,
  purgeWorkspace,
  streamWorkspaceInit,
  switchWorkspace,
  type WorkspaceStreamEvent,
} from './workspaceApi.ts';

type Fetch = typeof globalThis.fetch;

async function withFetch(impl: Fetch, run: () => Promise<void>): Promise<void> {
  const saved = globalThis.fetch;
  globalThis.fetch = impl;
  try {
    await run();
  } finally {
    globalThis.fetch = saved;
  }
}

const refused: Fetch = async () => {
  throw new TypeError('Failed to fetch');
};
const html500: Fetch = async () => new Response('<h1>Internal Server Error</h1>', { status: 500, headers: { 'Content-Type': 'text/html' } });

const calls: Array<[string, () => Promise<{ ok: boolean; error?: string }>]> = [
  ['fetchWorkspaceCurrent', () => fetchWorkspaceCurrent()],
  ['fetchWorkspaceRecent', () => fetchWorkspaceRecent()],
  ['browseWorkspace', () => browseWorkspace('/work')],
  ['switchWorkspace', () => switchWorkspace('/work')],
  ['fetchPurgePlan', () => fetchPurgePlan('/work')],
  ['purgeWorkspace', () => purgeWorkspace('/work', ['db'])],
  ['mkdirWorkspace', () => mkdirWorkspace('/work', 'x')],
];

for (const [name, call] of calls) {
  test(`${name} returns ok:false with an error when the connection fails`, async () => {
    await withFetch(refused, async () => {
      const res = await call();
      assert.equal(res.ok, false);
      assert.match(res.error ?? '', /server/i);
    });
  });

  test(`${name} returns ok:false with the status when the reply is not JSON`, async () => {
    await withFetch(html500, async () => {
      const res = await call();
      assert.equal(res.ok, false);
      assert.match(res.error ?? '', /500/);
    });
  });
}

test('a JSON error body is passed through with its message', async () => {
  await withFetch(async () => Response.json({ ok: false, error: 'not a directory' }, { status: 400 }), async () => {
    const res = await browseWorkspace('/nope');
    assert.equal(res.ok, false);
    assert.equal(res.error, 'not a directory');
  });
});

test('fetchWorkspaceRecent keeps an empty list on failure', async () => {
  await withFetch(refused, async () => {
    const res = await fetchWorkspaceRecent();
    assert.deepEqual(res.recent, []);
  });
});

function sseBody(frames: string[], fail = false): ReadableStream<Uint8Array> {
  const enc = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      for (const f of frames) controller.enqueue(enc.encode(f));
      if (fail) controller.error(new TypeError('network error'));
      else controller.close();
    },
  });
}

test('streamWorkspaceInit reports done when the stream ends without a done frame', async () => {
  const events: WorkspaceStreamEvent[] = [];
  await withFetch(async () => new Response(sseBody(['data: {"type":"line","text":"indexing"}\n\n'])), async () => {
    await streamWorkspaceInit('/work', (ev) => events.push(ev));
  });
  assert.deepEqual(events[0], { type: 'line', text: 'indexing' });
  const last = events.at(-1);
  assert.equal(last?.type, 'done');
  assert.equal(last?.type === 'done' && last.ok, false);
  assert.equal(events.filter((e) => e.type === 'done').length, 1);
});

test('streamWorkspaceInit reports done when the stream breaks', async () => {
  const events: WorkspaceStreamEvent[] = [];
  await withFetch(async () => new Response(sseBody(['data: {"type":"line","text":"a"}\n\n'], true)), async () => {
    await streamWorkspaceInit('/work', (ev) => events.push(ev));
  });
  assert.equal(events.at(-1)?.type, 'done');
});

test('streamWorkspaceInit reports done when the request fails', async () => {
  const events: WorkspaceStreamEvent[] = [];
  await withFetch(refused, async () => {
    await streamWorkspaceInit('/work', (ev) => events.push(ev));
  });
  assert.equal(events.length, 1);
  assert.equal(events[0].type === 'done' && events[0].ok, false);
});

test('streamWorkspaceInit sends a real done frame once', async () => {
  const events: WorkspaceStreamEvent[] = [];
  const frames = ['data: {"type":"line","text":"a"}\n\n', 'data: {"type":"done","ok":true,"initialized":true}\n\n'];
  await withFetch(async () => new Response(sseBody(frames)), async () => {
    await streamWorkspaceInit('/work', (ev) => events.push(ev));
  });
  assert.deepEqual(events, [
    { type: 'line', text: 'a' },
    { type: 'done', ok: true, initialized: true },
  ]);
});

test('streamWorkspaceInit keeps a final done frame that has no trailing blank line', async () => {
  const events: WorkspaceStreamEvent[] = [];
  const frames = ['data: {"type":"line","text":"a"}\r\n\r\n', 'data: {"type":"done","ok":true,"initialized":true}\n'];
  await withFetch(async () => new Response(sseBody(frames)), async () => {
    await streamWorkspaceInit('/work', (ev) => events.push(ev));
  });
  assert.deepEqual(events, [
    { type: 'line', text: 'a' },
    { type: 'done', ok: true, initialized: true },
  ]);
});
