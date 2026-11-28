import { ApiError, ensureOk } from './apiError.ts';

/**
 * Read a Server-Sent Events body and hand each event's data to `onData`.
 *
 * Events end at a blank line; LF, CRLF and CR line endings are all accepted.
 * A `data:` field may omit the space after the colon, and several data lines
 * in one event are joined with a newline. When the stream closes, a last event
 * that has no trailing blank line is still delivered. Reading stops early once
 * `live()` returns false.
 */
export async function readSseData(
  body: ReadableStream<Uint8Array>,
  onData: (data: string) => void,
  live: () => boolean = () => true,
): Promise<void> {
  const reader = body.getReader();
  const dec = new TextDecoder();
  let raw = '';
  let buf = '';

  const dispatch = (block: string) => {
    const data: string[] = [];
    for (const line of block.split('\n')) {
      if (line === '' || line.startsWith(':')) continue;
      const colon = line.indexOf(':');
      const field = colon < 0 ? line : line.slice(0, colon);
      if (field !== 'data') continue;
      let value = colon < 0 ? '' : line.slice(colon + 1);
      if (value.startsWith(' ')) value = value.slice(1);
      data.push(value);
    }
    if (data.length > 0) onData(data.join('\n'));
  };

  /** Returns false when the reader should stop. */
  const feed = (text: string, final: boolean): boolean => {
    raw += text;
    // A CR at the end may be the first half of a CRLF split across chunks.
    const keep = !final && raw.endsWith('\r') ? 1 : 0;
    buf += raw.slice(0, raw.length - keep).replace(/\r\n?/g, '\n');
    raw = raw.slice(raw.length - keep);
    let end: number;
    while ((end = buf.indexOf('\n\n')) >= 0) {
      if (!live()) return false;
      const block = buf.slice(0, end);
      buf = buf.slice(end + 2);
      dispatch(block);
    }
    if (final && buf.trim() !== '') {
      if (!live()) return false;
      const block = buf;
      buf = '';
      dispatch(block);
    }
    return true;
  };

  for (;;) {
    if (!live()) break;
    const { done, value } = await reader.read();
    if (done) {
      feed(dec.decode(), true);
      return;
    }
    if (!feed(dec.decode(value, { stream: true }), false)) break;
  }
  await reader.cancel().catch(() => {});
}

/** {@link readSseData} for events whose data is JSON; data that does not parse is skipped. */
export async function readSseJson<T>(
  body: ReadableStream<Uint8Array>,
  onEvent: (event: T) => void,
  live?: () => boolean,
): Promise<void> {
  await readSseData(
    body,
    (data) => {
      let event: T;
      try {
        event = JSON.parse(data) as T;
      } catch {
        return;
      }
      onEvent(event);
    },
    live,
  );
}

/**
 * POST `body` as JSON to an event-stream endpoint and return the open stream.
 * A non-2xx reply throws an {@link ApiError} with the status and the server's
 * message; a reply without a body throws one too.
 */
export async function openSse(url: string, body: unknown, signal?: AbortSignal): Promise<ReadableStream<Uint8Array>> {
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body ?? {}),
    signal,
  });
  await ensureOk(res);
  if (!res.body) throw new ApiError(`The server sent no event stream (HTTP ${res.status})`, res.status);
  return res.body;
}

/** {@link openSse} and then {@link readSseJson}: errors reject, events go to `onEvent`. */
export async function postSse<T>(
  url: string,
  body: unknown,
  onEvent: (event: T) => void,
  signal?: AbortSignal,
): Promise<void> {
  await readSseJson<T>(await openSse(url, body, signal), onEvent);
}
