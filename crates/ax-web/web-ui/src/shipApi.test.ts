import assert from 'node:assert/strict';
import { afterEach, describe, it } from 'node:test';

import { updateShipConfig, type ShipConfig } from './shipApi.ts';

const stored: ShipConfig = {
  ship: { target_branch: 'release', web_port: 7070 },
  quality_gate: { steps: ['tests'], tests: { runner: 'make check' } },
  remote: { provider: 'github', github: { owner: 'acme', repo: 'core', token_env: 'GH' } },
  sonar: {
    enabled: true,
    host: 'http://127.0.0.1:9000',
    project_key: 'acme',
    token_env: 'SONAR_TOKEN',
    scanner_path: 'sonar-scanner',
    container_runtime: 'podman',
  },
  ui: { show_savings: true, timezone: 'Europe/Amsterdam' },
  reviewers: { lead: 'kim' },
};

const realFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = realFetch;
});

function serve(getStatus = 200): { puts: unknown[] } {
  const seen = { puts: [] as unknown[] };
  globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    assert.equal(String(input), '/api/ship/config');
    if (init?.method === 'PUT') {
      seen.puts.push(JSON.parse(String(init.body)));
      return new Response(JSON.stringify({ ok: true }), { status: 200 });
    }
    const body = getStatus === 200 ? { config: stored, sonar: {} } : { error: 'server restarting' };
    return new Response(JSON.stringify(body), { status: getStatus });
  }) as typeof fetch;
  return seen;
}

describe('updateShipConfig', () => {
  it('changes one field of the stored config and keeps the rest', async () => {
    const seen = serve();
    const saved = await updateShipConfig((c) => ({ ...c, ui: { ...c.ui, show_savings: false } }));
    assert.equal(seen.puts.length, 1);
    assert.deepEqual(seen.puts[0], { ...stored, ui: { show_savings: false, timezone: 'Europe/Amsterdam' } });
    assert.deepEqual(saved, seen.puts[0]);
  });

  it('writes nothing when the stored config cannot be read', async () => {
    const seen = serve(503);
    await assert.rejects(updateShipConfig((c) => c), /server restarting/);
    assert.equal(seen.puts.length, 0);
  });
});
