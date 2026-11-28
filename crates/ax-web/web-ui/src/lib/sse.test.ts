import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readSseData, readSseJson } from './sse.ts';

function body(chunks: string[]): ReadableStream<Uint8Array> {
  const enc = new TextEncoder();
  return new ReadableStream({
    start(c) {
      for (const chunk of chunks) c.enqueue(enc.encode(chunk));
      c.close();
    },
  });
}

async function collect(chunks: string[]): Promise<string[]> {
  const out: string[] = [];
  await readSseData(body(chunks), (d) => out.push(d));
  return out;
}

test('a CRLF split across two chunks is one line break, not two', async () => {
  assert.deepEqual(await collect(['data: a\r', '\ndata: b\r\n\r\n']), ['a\nb']);
});

test('bare CR line endings end an event', async () => {
  assert.deepEqual(await collect(['data: a\r\rdata: b\r\r']), ['a', 'b']);
});

test('several data lines in one event are joined with a newline', async () => {
  assert.deepEqual(await collect(['data: one\ndata: two\n\n']), ['one\ntwo']);
});

test('other fields and comments are ignored', async () => {
  assert.deepEqual(await collect(['event: x\nid: 1\n: note\ndata: v\n\nretry: 5\n\n']), ['v']);
});

test('only one space after the colon is removed', async () => {
  assert.deepEqual(await collect(['data:  two spaces\n\n']), [' two spaces']);
});

test('reading stops once the caller is no longer live', async () => {
  const seen: number[] = [];
  let live = true;
  await readSseJson<{ n: number }>(
    body(['data: {"n":1}\n\ndata: {"n":2}\n\n', 'data: {"n":3}\n\n']),
    (ev) => {
      seen.push(ev.n);
      if (ev.n === 1) live = false;
    },
    () => live,
  );
  assert.deepEqual(seen, [1]);
});
