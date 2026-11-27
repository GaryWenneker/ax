import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import type { AgentTargetStatus, AgentsConfig } from '../../agentApi.ts';
import { chatFailureLine, initialAgentPlan, profileSwitchLine } from './agentTerminalModel.ts';

const target = (over: Partial<AgentTargetStatus>): AgentTargetStatus => ({
  id: 'claude', display_name: 'Claude Code', detected: true, cli_on_path: true, cli_installable: true,
  configured: true, config_paths: [], runnable: true, ...over,
});

const source = readFileSync(new URL('./AgentTerminal.tsx', import.meta.url), 'utf8');

test('the initial agent label comes from the fetched catalog', () => {
  const config = { last_terminal_agent: 'claude', active_profile: { claude: 'work' } } as unknown as AgentsConfig;
  const plan = initialAgentPlan(config, [target({ display_name: 'Claude (fetched)' })]);
  assert.deepEqual(plan, { agent: 'claude', profileId: 'work', notice: 'Interactive Claude (fetched) CLI · profile work' });
});

test('a missing CLI gives the install notice', () => {
  const config = { last_terminal_agent: 'claude' } as unknown as AgentsConfig;
  const plan = initialAgentPlan(config, [target({ cli_on_path: false })]);
  assert.equal(plan.notice, 'Claude Code CLI not installed — install it manually, then select Claude Code again.');
});

test('the built-in agent has no notice', () => {
  const config = { terminal_mode: 'builtin' } as unknown as AgentsConfig;
  assert.deepEqual(initialAgentPlan(config, []), { agent: 'builtin', profileId: 'default', notice: null });
});

test('the mount effect does not depend on the catalog, so a fetch cannot re-run it', () => {
  const effect = source.match(/const data = await refreshConfig\(\);[\s\S]*?\}, \[([^\]]*)\]\);/);
  assert.ok(effect, 'mount effect not found');
  assert.deepEqual(effect[1].split(',').map(s => s.trim()).sort(), ['pushSystem', 'refreshConfig']);
});

test('a failed chat request reports the error, an aborted one stays quiet', () => {
  assert.equal(chatFailureLine(new TypeError('Failed to fetch')), 'Chat request failed: Failed to fetch');
  assert.equal(chatFailureLine(new DOMException('The operation was aborted.', 'AbortError')), null);
  assert.equal(chatFailureLine('boom'), 'Chat request failed: boom');
});

test('a profile switch reports what the server answered', () => {
  assert.equal(profileSwitchLine({ ok: true }, 'work', 'claude'), 'Switched profile to work (claude)');
  assert.equal(profileSwitchLine({ ok: false, error: 'unknown profile' }, 'work', 'claude'), 'Could not switch profile to work: unknown profile');
  assert.equal(profileSwitchLine({ ok: false }, 'work', 'claude'), 'Could not switch profile to work');
});

test('the terminal catches send and profile failures and aborts the stream on unmount', () => {
  assert.match(source, /catch \(e\) \{\s*const line = chatFailureLine\(e\)/);
  assert.match(source, /profileSwitchLine\(/);
  assert.match(source, /return \(\) => \{\s*abortRef\.current\?\.abort\(\);/);
});

test('saving a profile always clears the busy flag', () => {
  const editor = readFileSync(new URL('./ProfileEditor.tsx', import.meta.url), 'utf8');
  assert.match(editor, /try \{[\s\S]*updateAgentProfile[\s\S]*\} catch \(e\) \{[\s\S]*\} finally \{\s*setSaving\(false\);/);
});

test('a failed agent preference save is reported, not left as an unhandled rejection', () => {
  const persist = source.slice(source.indexOf('async function persistAgentChoice('), source.indexOf('async function onAgentChange('));
  assert.match(persist, /try \{[\s\S]*saveAgentConfig[\s\S]*\} catch \(e\) \{[\s\S]*pushSystem\(/);
});
