// Runs with `node --test` (Node strips the TypeScript types).
import assert from 'node:assert/strict';
import { afterEach, test } from 'node:test';
import worker from '../src/index.ts';

const MACHINE = '0f8fad5b-d9cb-469f-a165-70867728950e';
const realFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = realFetch;
});

/** Posts one batch and returns the status plus the events forwarded to PostHog. */
async function post(body: unknown): Promise<{ status: number; forwarded: Array<{ event: string }> }> {
  const forwarded: Array<{ event: string }> = [];
  globalThis.fetch = (async (_url: unknown, init?: { body?: string }) => {
    forwarded.push(...JSON.parse(String(init?.body)).batch);
    return new Response(null, { status: 200 });
  }) as typeof fetch;
  const pending: Promise<unknown>[] = [];
  const env = {
    POSTHOG_HOST: 'https://posthog.invalid',
    POSTHOG_KEY: 'key',
    MACHINE_RATE_LIMITER: { limit: async () => ({ success: true }) },
  };
  const ctx = { waitUntil: (p: Promise<unknown>) => pending.push(p) };
  const text = JSON.stringify(body);
  const request = new Request('https://telemetry.invalid/v1/events', {
    method: 'POST',
    headers: { 'content-length': String(Buffer.byteLength(text)), 'content-type': 'application/json' },
    body: text,
  });
  const res = await (worker as any).fetch(request, env, ctx);
  await Promise.all(pending);
  return { status: res.status, forwarded };
}

const install = { event: 'install', props: { scope: 'local', kind: 'fresh' } };

test('forwards a valid event', async () => {
  const { status, forwarded } = await post({ machine_id: MACHINE, events: [install] });
  assert.equal(status, 204);
  assert.deepEqual(forwarded.map((e) => e.event), ['install']);
});

for (const name of ['toString', 'constructor', '__proto__', 'hasOwnProperty', 'valueOf']) {
  test(`drops an event named after Object.prototype member '${name}' and keeps the rest of the batch`, async () => {
    const { status, forwarded } = await post({ machine_id: MACHINE, events: [{ event: name }, install] });
    assert.equal(status, 204);
    assert.deepEqual(forwarded.map((e) => e.event), ['install']);
  });
}
