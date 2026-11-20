import assert from 'node:assert/strict';
import test from 'node:test';

import { continuityPrompt, copyIdentifier, filledContinuityFieldsFromEntry, filledContinuityFieldsFromTrace, logRowSelection, testMode, type SessionIdentity, type TestReference } from './continuity.ts';

const identity: SessionIdentity = {
  externalChatId: '  ',
  externalTurnId: null,
  axSessionId: 'axs_source',
  axTurnId: null,
  agentProvider: null,
  projectId: 'ax',
  createdAt: '2026-11-20T09:00:00Z',
  parentSessionId: null,
};

const reference: TestReference = {
  id: 'ctr-1',
  sourceSessionId: 'axs_source',
  checkpointId: 'cp-1',
  mode: 'historical',
  task: 'Continue the unfinished feature.',
  targetAgent: 'cursor',
};

test('missing provider ids are not invented', () => {
  const chat = copyIdentifier(identity, 'chat');
  assert.equal(chat.ok, false);
  if (!chat.ok) assert.equal(chat.error, 'external chat id was not issued');
  const all = copyIdentifier(identity, 'all');
  assert.equal(all.ok, true);
  if (all.ok) {
    assert.match(all.text, /external_chat_id: not issued/);
    assert.match(all.text, /ax_session_id: axs_source/);
    assert.equal(all.text.includes('axs_minted'), false);
    assert.equal(all.text.includes('chat-'), false);
  }
});

test('the continuity prompt names the reference and not the answer', () => {
  const prompt = continuityPrompt(reference);
  assert.equal(prompt.ok, true);
  if (prompt.ok) {
    assert.match(prompt.text, /ctr-1/);
    assert.match(prompt.text, /cp-1/);
    assert.equal(prompt.text.includes('secret historical answer'), false);
    assert.equal(prompt.text.includes('BEGIN TRANSCRIPT'), false);
    assert.match(prompt.text, /Spec: none/);
  }
  const pasted = continuityPrompt({ ...reference, task: 'BEGIN TRANSCRIPT pasted' });
  assert.equal(pasted.ok, false);
  const linked = continuityPrompt({ ...reference, specId: 'billing', specVersion: 2, specHash: 'abc' });
  assert.equal(linked.ok, true);
  if (linked.ok) {
    assert.match(linked.text, /Spec: billing@2 abc/);
    assert.match(linked.text, /Pointer: ax spec show billing@2/);
    assert.equal(linked.text.includes('Keep the first price.'), false);
  }
});

test('an unknown mode stays on the historical checkpoint', () => {
  assert.equal(testMode('current'), 'current');
  assert.equal(testMode('historical'), 'historical');
  assert.equal(testMode('later'), 'historical');
});

test('a placeholder project id is not issued', () => {
  const dashed = copyIdentifier({ ...identity, projectId: '—' }, 'all');
  assert.equal(dashed.ok, true);
  if (dashed.ok) assert.match(dashed.text, /project_id: not issued/);
});

test('continuity fields stay filled from the live trace', () => {
  const filled = filledContinuityFieldsFromTrace([
    { sessionId: 'older', turnId: 't0', time: '2026-11-20T08:00:00Z', instantMs: 1, tool: 'cursor', message: 'Earlier task' },
    { sessionId: 'axs_source', turnId: 'turn-9', time: '2026-11-20T09:00:00Z', instantMs: 2, tool: 'cursor', message: 'Continue the unfinished feature.' },
  ], '/Users/gary/io/ax');
  assert.equal(filled.projectId, '/Users/gary/io/ax');
  assert.equal(filled.axSessionId, 'axs_source');
  assert.equal(filled.externalChatId, 'axs_source');
  assert.equal(filled.axTurnId, 'turn-9');
  assert.equal(filled.externalTurnId, 'turn-9');
  assert.equal(filled.createdAt, '2026-11-20T09:00:00Z');
  assert.equal(filled.agentProvider, 'cursor');
  assert.equal(filled.targetAgent, 'cursor');
  assert.equal(filled.parentSessionId, 'older');
  assert.equal(filled.checkpointId, 'cp-turn-9');
  assert.equal(filled.task, 'Continue the unfinished feature.');
  for (const value of Object.values(filled)) assert.equal(String(value).trim().length > 0, true);
});

test('continuity fields stay filled when the trace has no ids yet', () => {
  const filled = filledContinuityFieldsFromTrace([], '');
  for (const value of Object.values(filled)) assert.equal(String(value).trim().length > 0, true);
});

test('a clicked log row fills continuity from that row', () => {
  const older = { sessionId: 'older', turnId: 't0', time: '2026-11-20T08:00:00Z', instantMs: 1, tool: 'cursor', message: 'Earlier task' };
  const clicked = { sessionId: 'clicked-session', turnId: 'turn-clicked', time: '2026-11-20T07:00:00Z', instantMs: 0, tool: 'ax_guard', message: 'Use the selected log line.' };
  const newest = { sessionId: 'axs_source', turnId: 'turn-9', time: '2026-11-20T09:00:00Z', instantMs: 2, tool: 'cursor', message: 'Continue the unfinished feature.' };
  const filled = filledContinuityFieldsFromEntry(clicked, [older, clicked, newest], '/Users/gary/io/ax');
  assert.equal(filled.axSessionId, 'clicked-session');
  assert.equal(filled.externalChatId, 'clicked-session');
  assert.equal(filled.axTurnId, 'turn-clicked');
  assert.equal(filled.externalTurnId, 'turn-clicked');
  assert.equal(filled.createdAt, '2026-11-20T07:00:00Z');
  assert.equal(filled.agentProvider, 'ax_guard');
  assert.equal(filled.targetAgent, 'ax_guard');
  assert.equal(filled.checkpointId, 'cp-turn-clicked');
  assert.equal(filled.task, 'Use the selected log line.');
  assert.equal(filled.projectId, '/Users/gary/io/ax');
  assert.equal(filled.parentSessionId, 'older');
  assert.equal(filled.mode, 'historical');
});

test('a log click opens the side panel only while continuity is closed', () => {
  assert.equal(logRowSelection(null, 'row-1', true), null);
  assert.equal(logRowSelection('row-0', 'row-1', true), null);
  assert.equal(logRowSelection(null, 'row-1', false), 'row-1');
  assert.equal(logRowSelection('row-1', 'row-1', false), null);
});

test('copying the test reference does not embed a historical answer', () => {
  const copied = copyIdentifier(identity, 'reference', reference);
  assert.equal(copied.ok, true);
  if (copied.ok) {
    assert.match(copied.text, /checkpoint_id: cp-1/);
    assert.equal(copied.text.includes('secret historical answer'), false);
  }
});
