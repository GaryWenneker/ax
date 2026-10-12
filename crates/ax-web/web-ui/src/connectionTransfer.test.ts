import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseConnectionDetails } from './connectionTransfer.ts';
const connection = { kind: 'axRemoteConnection', version: 1, projectMcpUrl: 'https://ax.example/projects/demo/mcp', nativeClientId: 'native-public' };
test('connection details contain only a project URL and the native public sign-in ID', () => {
  assert.deepEqual(parseConnectionDetails(JSON.stringify(connection)), connection);
  assert.equal(parseConnectionDetails(JSON.stringify({ ...connection, nativeClientId: null })).nativeClientId, null);
});
test('server files, IDE registrations, secrets and invalid project URLs are not accepted', () => {
  for (const value of [{ ...connection, accessToken: 'secret' }, { ...connection, kind: 'mcpServers' }, { public_url: 'https://server.example' }, { ...connection, nativeClientId: '\n' }, { ...connection, version: 2 }, { ...connection, projectMcpUrl: 'http://ax.example/projects/demo/mcp' }, { ...connection, projectMcpUrl: 'https://user:secret@ax.example/projects/demo/mcp' }, { ...connection, projectMcpUrl: 'https://ax.example/projects/demo/mcp?token=secret' }, { ...connection, projectMcpUrl: 'https://ax.example/health' }]) {
    assert.throws(() => parseConnectionDetails(JSON.stringify(value)));
  }
  assert.throws(() => parseConnectionDetails('not JSON'));
  assert.throws(() => parseConnectionDetails('x'.repeat(17 * 1024)));
});
