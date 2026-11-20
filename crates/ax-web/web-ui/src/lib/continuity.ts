export type TestMode = 'historical' | 'current';

export type SessionIdentity = {
  externalChatId: string | null;
  externalTurnId: string | null;
  axSessionId: string | null;
  axTurnId: string | null;
  agentProvider: string | null;
  projectId: string | null;
  createdAt: string | null;
  parentSessionId: string | null;
};

export type CopyKind = 'chat' | 'turn' | 'ax-session' | 'ax-turn' | 'all' | 'reference';

export type TestReference = {
  id: string;
  sourceSessionId: string;
  checkpointId: string;
  mode: TestMode;
  task: string;
  targetAgent: string;
  specId?: string | null;
  specVersion?: number | null;
  specHash?: string | null;
};

const NOT_ISSUED = 'not issued';
const PLACEHOLDERS = new Set(['—', '-', '…', '...']);

function present(value: string | null | undefined): string | null {
  const trimmed = (value ?? '').trim();
  if (trimmed.length === 0 || PLACEHOLDERS.has(trimmed)) return null;
  return trimmed;
}

export function testMode(value: string): TestMode {
  return value === 'current' ? 'current' : 'historical';
}

export function copyIdentifier(
  identity: SessionIdentity,
  kind: CopyKind,
  reference?: TestReference,
): { ok: true; text: string } | { ok: false; error: string } {
  if (kind === 'chat') return issued(identity.externalChatId, 'external chat id');
  if (kind === 'turn') return issued(identity.externalTurnId, 'external turn id');
  if (kind === 'ax-session') {
    const id = present(identity.axSessionId);
    return id ? { ok: true, text: id } : { ok: false, error: 'ax session id is required' };
  }
  if (kind === 'ax-turn') return issued(identity.axTurnId, 'ax turn id');
  if (kind === 'all') return { ok: true, text: renderAll(identity) };
  if (!reference || present(reference.id) == null || present(reference.checkpointId) == null) {
    return { ok: false, error: 'context test reference is missing' };
  }
  return { ok: true, text: renderReference(reference) };
}

function issued(value: string | null, label: string): { ok: true; text: string } | { ok: false; error: string } {
  const id = present(value);
  return id ? { ok: true, text: id } : { ok: false, error: `${label} was ${NOT_ISSUED}` };
}

function line(label: string, value: string | null): string {
  return `${label}: ${present(value) ?? NOT_ISSUED}`;
}

function renderAll(identity: SessionIdentity): string {
  return [
    line('external_chat_id', identity.externalChatId),
    line('external_turn_id', identity.externalTurnId),
    line('ax_session_id', identity.axSessionId),
    line('ax_turn_id', identity.axTurnId),
    line('agent_provider', identity.agentProvider),
    line('project_id', identity.projectId),
    line('created_at', identity.createdAt),
    line('parent_session_id', identity.parentSessionId),
  ].join('\n');
}

function renderReference(reference: TestReference): string {
  return [
    `reference_id: ${reference.id}`,
    `source_session_id: ${reference.sourceSessionId}`,
    `checkpoint_id: ${reference.checkpointId}`,
    `mode: ${reference.mode}`,
    `target_agent: ${reference.targetAgent}`,
    `task: ${reference.task}`,
  ].join('\n');
}

export function continuityPrompt(reference: TestReference): { ok: true; text: string } | { ok: false; error: string } {
  if (present(reference.id) == null || present(reference.checkpointId) == null) {
    return { ok: false, error: 'a continuity prompt needs a reference id and a checkpoint id' };
  }
  if (reference.task.includes('BEGIN TRANSCRIPT')) {
    return { ok: false, error: 'the task must not carry a transcript' };
  }
  return {
    ok: true,
    text: [
      `Retrieve context through Ax for continuity test ${reference.id}.`,
      `Source session: ${reference.sourceSessionId}`,
      `Checkpoint: ${reference.checkpointId}`,
      `Mode: ${reference.mode}`,
      `Target agent: ${reference.targetAgent}`,
      `Task: ${reference.task}`,
      ...specLines(reference),
      'Use normal Ax retrieval for this reference. Historical answers are not included.',
    ].join('\n'),
  };
}

function specLines(reference: TestReference): string[] {
  const id = present(reference.specId);
  const hash = present(reference.specHash);
  if (id && reference.specVersion != null && hash) {
    return [`Spec: ${id}@${reference.specVersion} ${hash}`, `Pointer: ax spec show ${id}@${reference.specVersion}`];
  }
  return ['Spec: none'];
}

export type ContinuityDraft = {
  externalChatId: string;
  externalTurnId: string;
  axSessionId: string;
  axTurnId: string;
  agentProvider: string;
  projectId: string;
  createdAt: string;
  parentSessionId: string;
  checkpointId: string;
  mode: TestMode;
  targetAgent: string;
  task: string;
};

type TraceFact = {
  sessionId?: string | null;
  turnId?: string | null;
  time?: string;
  instantMs?: number | null;
  tool?: string | null;
  message?: string;
};

function text(value: string | null | undefined): string {
  return (value ?? '').trim();
}

function taskLine(message: string | undefined): string {
  const line = text(message).split('\n')[0] ?? '';
  if (line.length === 0 || line.includes('BEGIN TRANSCRIPT')) return '';
  return line.slice(0, 180);
}

/** Every continuity field is non-empty. Issued ids win; a missing id keeps a filled stand-in. */
export function filledContinuityFieldsFromTrace(entries: TraceFact[], projectId: string): ContinuityDraft {
  const ranked = [...entries].sort((left, right) => (right.instantMs ?? -1) - (left.instantMs ?? -1));
  const project = text(projectId) || 'ax';
  const sessionId = ranked.map((entry) => text(entry.sessionId)).find((id) => id.length > 0) || project;
  const turnId = ranked.map((entry) => text(entry.turnId)).find((id) => id.length > 0) || sessionId;
  const createdAt = ranked.map((entry) => text(entry.time)).find((time) => time.length > 0) || '1970-01-01T00:00:00Z';
  const provider = ranked.map((entry) => text(entry.tool)).find((tool) => tool.length > 0) || 'agent';
  const parentSessionId = ranked.map((entry) => text(entry.sessionId)).find((id) => id.length > 0 && id !== sessionId) || sessionId;
  const task = ranked.map((entry) => taskLine(entry.message)).find((line) => line.length > 0) || 'Continue the current task.';
  return {
    externalChatId: sessionId,
    externalTurnId: turnId,
    axSessionId: sessionId,
    axTurnId: turnId,
    agentProvider: provider,
    projectId: project,
    createdAt,
    parentSessionId,
    checkpointId: `cp-${turnId}`,
    mode: 'historical',
    targetAgent: provider,
    task,
  };
}

function parentSession(ordered: TraceFact[], index: number, sessionId: string): string {
  for (let i = index + 1; i < ordered.length; i += 1) {
    const id = text(ordered[i]?.sessionId);
    if (id.length > 0 && id !== sessionId) return id;
  }
  for (let i = index - 1; i >= 0; i -= 1) {
    const id = text(ordered[i]?.sessionId);
    if (id.length > 0 && id !== sessionId) return id;
  }
  return sessionId;
}

/** Fields for one clicked log row. Other rows supply only a different parent session. */
export function filledContinuityFieldsFromEntry(entry: TraceFact, entries: TraceFact[], projectId: string): ContinuityDraft {
  const project = text(projectId) || 'ax';
  const sessionId = text(entry.sessionId) || project;
  const turnId = text(entry.turnId) || sessionId;
  const ordered = [...entries].sort((left, right) => (right.instantMs ?? -1) - (left.instantMs ?? -1));
  const index = Math.max(0, ordered.indexOf(entry));
  return {
    externalChatId: sessionId,
    externalTurnId: turnId,
    axSessionId: sessionId,
    axTurnId: turnId,
    agentProvider: text(entry.tool) || 'agent',
    projectId: project,
    createdAt: text(entry.time) || '1970-01-01T00:00:00Z',
    parentSessionId: parentSession(ordered, index, sessionId),
    checkpointId: `cp-${turnId}`,
    mode: 'historical',
    targetAgent: text(entry.tool) || 'agent',
    task: taskLine(entry.message) || 'Continue the current task.',
  };
}

/** A log-row click opens the side panel only while the continuity panel is closed. */
export function logRowSelection(current: string | null, rowId: string, continuityOpen: boolean): string | null {
  if (continuityOpen) return null;
  return current === rowId ? null : rowId;
}
