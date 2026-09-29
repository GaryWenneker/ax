import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { AgentTargetStatus } from './agentApi.ts';
import { connectAllText, foundNotConnected, panelLabel, ideResultText, ideState } from './ideInstall.ts';

const row = (id: string, detected: boolean, configured: boolean): AgentTargetStatus => ({
  id,
  display_name: id,
  detected,
  cli_on_path: false,
  cli_installable: false,
  configured,
  config_paths: [],
});

test('an IDE is connected, found, or not found', () => {
  assert.equal(ideState(row('cursor', true, true)), 'connected');
  assert.equal(ideState(row('cursor', false, true)), 'connected');
  assert.equal(ideState(row('cursor', true, false)), 'found');
  assert.equal(ideState(row('cursor', false, false)), 'missing');
});

test('connect all picks found IDEs that are not connected', () => {
  const list = [row('a', true, true), row('b', true, false), row('c', false, false), row('d', true, false)];
  assert.deepEqual(foundNotConnected(list), ['b', 'd']);
});

test('the result names each IDE, its files, and its notes', () => {
  const text = ideResultText('Connected', [
    { id: 'vscode', display_name: 'VS Code', files: ['/p/.vscode/mcp.json', '/h/.claude/settings.json'], notes: ['Reload the window.'] },
    { id: 'zed', display_name: 'Zed', files: [], notes: [] },
  ]);
  assert.equal(
    text,
    'Connected VS Code: /p/.vscode/mcp.json, /h/.claude/settings.json. Reload the window.\nConnected Zed: already up to date.',
  );
  assert.equal(ideResultText('Disconnected', []), 'Nothing changed.');
});

test('connectAllText says loading until status arrives, never all-connected', () => {
  assert.equal(connectAllText('loading', 0), 'Loading IDEs…');
  assert.equal(connectAllText('error', 0), 'Could not load IDEs.');
  assert.equal(connectAllText('ready', 0), 'Every IDE found here is connected.');
  assert.equal(connectAllText('ready', 2), '2 found but not connected.');
});

test('panelLabel shows whether the Command Center is in the IDE, nothing for terminal agents', () => {
  assert.equal(panelLabel({ ...row('cursor', true, true), panel: true }), 'Panel');
  assert.equal(panelLabel({ ...row('cursor', true, true), panel: false }), 'No panel');
  assert.equal(panelLabel({ ...row('claude', true, true), panel: null }), null);
  assert.equal(panelLabel(row('claude', true, true)), null);
});
