import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

import {
  continuityPrompt,
  copyIdentifier,
  mergeContinuityDraft,
  testMode,
  type ContinuityDraft,
  type SessionIdentity,
  type TestReference,
} from '../lib/continuity';

type Props = {
  values: ContinuityDraft;
  onOpenChange?: (open: boolean) => void;
};

function keep(current: string, next: string): string {
  return next.trim().length > 0 ? next : current;
}

export default function ContinuityPanel({ values, onOpenChange }: Props) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState(values);
  const [notice, setNotice] = useState('Copy an id, or prepare a prompt for a new chat.');
  const [toolbar, setToolbar] = useState<HTMLElement | null>(null);

  useLayoutEffect(() => {
    setToolbar(document.getElementById('continuity-toolbar-slot'));
  }, []);

  // Derived values are rebuilt on every live log event; keep what the user typed.
  const edited = useRef<Set<keyof ContinuityDraft>>(new Set());
  const lastValues = useRef(values);
  useEffect(() => {
    if (lastValues.current === values) return;
    const previous = lastValues.current;
    lastValues.current = values;
    setDraft((current) => {
      const merged = mergeContinuityDraft(current, values, previous, edited.current);
      edited.current = merged.edited;
      return merged.draft;
    });
  }, [values]);

  useEffect(() => {
    onOpenChange?.(open);
  }, [open, onOpenChange]);

  const identity = useMemo<SessionIdentity>(() => ({
    externalChatId: draft.externalChatId,
    externalTurnId: draft.externalTurnId,
    axSessionId: draft.axSessionId,
    axTurnId: draft.axTurnId,
    agentProvider: draft.agentProvider,
    projectId: draft.projectId,
    createdAt: draft.createdAt,
    parentSessionId: draft.parentSessionId,
  }), [draft]);

  const [boundSpec, setBoundSpec] = useState<{ id: string; version: number; hash: string } | null>(null);

  useEffect(() => {
    const session = draft.axSessionId.trim();
    if (!session) {
      setBoundSpec(null);
      return;
    }
    let cancelled = false;
    void fetch('/api/specs')
      .then((response) => response.json())
      .then((body: { specs?: { id: string; sessions?: string; pointerVersion?: number | null; pointerHash?: string | null }[] }) => {
        if (cancelled) return;
        const match = (body.specs ?? []).find((spec) => (spec.sessions ?? '').split(/\s+/).filter(Boolean).includes(session));
        if (match && match.pointerVersion != null && match.pointerHash) {
          setBoundSpec({ id: match.id, version: match.pointerVersion, hash: match.pointerHash });
        } else {
          setBoundSpec(null);
        }
      })
      .catch(() => {
        if (!cancelled) setBoundSpec(null);
      });
    return () => {
      cancelled = true;
    };
  }, [draft.axSessionId]);

  const reference = useMemo<TestReference>(() => ({
    id: `ctr-${draft.axSessionId}-${draft.checkpointId}`,
    sourceSessionId: draft.axSessionId,
    checkpointId: draft.checkpointId,
    mode: draft.mode,
    task: draft.task,
    targetAgent: draft.targetAgent,
    specId: boundSpec?.id ?? null,
    specVersion: boundSpec?.version ?? null,
    specHash: boundSpec?.hash ?? null,
  }), [draft, boundSpec]);

  const generated = useMemo(() => {
    const result = continuityPrompt(reference);
    return result.ok ? result.text : '';
  }, [reference]);
  const [prompt, setPrompt] = useState(generated);
  const [promptEdited, setPromptEdited] = useState(false);

  useEffect(() => {
    if (!promptEdited) setPrompt(generated);
  }, [generated, promptEdited]);

  function setField<K extends keyof ContinuityDraft>(key: K, next: ContinuityDraft[K]) {
    edited.current.add(key);
    setDraft((current) => ({ ...current, [key]: typeof next === 'string' ? keep(String(current[key]), next) : next }));
  }

  async function copy(kind: 'chat' | 'turn' | 'ax-session' | 'all' | 'reference') {
    const result = copyIdentifier(identity, kind, reference);
    if (!result.ok) {
      setNotice(result.error);
      return;
    }
    try {
      await navigator.clipboard.writeText(result.text);
    } catch {
      setNotice(`${result.text}\nClipboard access was denied. Select this text and copy it manually.`);
      return;
    }
    setNotice(`Copied ${kind}.`);
  }

  function preparePrompt() {
    const result = continuityPrompt(reference);
    if (!result.ok) {
      setNotice(result.error);
      return;
    }
    setPrompt(result.text);
    setPromptEdited(false);
    setNotice('Prompt ready.');
  }

  async function copyPrompt() {
    if (prompt.trim().length === 0) {
      setNotice('Prepare a prompt first.');
      return;
    }
    try {
      await navigator.clipboard.writeText(prompt);
    } catch {
      const box = document.getElementById('continuity-prompt-text');
      if (box instanceof HTMLTextAreaElement) {
        box.focus();
        box.select();
      }
      setNotice('Clipboard access was denied. The prompt is selected; copy it manually.');
      return;
    }
    setNotice('Copied prompt.');
  }

  const toggle = (
    <button type="button" className={`btn btn-compact mcp-trace-icon-btn continuity-toggle${open ? ' mcp-trace-icon-btn--active' : ''}`} aria-expanded={open} aria-controls="continuity-fields" onClick={() => setOpen((current) => !current)}>
      <span className="mcp-trace-btn-label">Context continuity</span>
    </button>
  );

  return (
    <section className={`continuity-panel${toolbar ? ' continuity-panel--docked' : ''}`} aria-label="Context continuity">
      {toolbar ? createPortal(toggle, toolbar) : toggle}
      <div id="continuity-fields" className="continuity-body" hidden={!open}>
        <div className="continuity-grid">
          <label><span>External chat id</span><input value={draft.externalChatId} onChange={(event) => setField('externalChatId', event.target.value)} /></label>
          <label><span>External turn id</span><input value={draft.externalTurnId} onChange={(event) => setField('externalTurnId', event.target.value)} /></label>
          <label><span>Ax session id</span><input value={draft.axSessionId} onChange={(event) => setField('axSessionId', event.target.value)} /></label>
          <label><span>Ax turn id</span><input value={draft.axTurnId} onChange={(event) => setField('axTurnId', event.target.value)} /></label>
          <label><span>Agent provider</span><input value={draft.agentProvider} onChange={(event) => setField('agentProvider', event.target.value)} /></label>
          <label><span>Project id</span><input value={draft.projectId} onChange={(event) => setField('projectId', event.target.value)} /></label>
          <label><span>Created at</span><input value={draft.createdAt} onChange={(event) => setField('createdAt', event.target.value)} /></label>
          <label><span>Parent session</span><input value={draft.parentSessionId} onChange={(event) => setField('parentSessionId', event.target.value)} /></label>
          <label><span>Checkpoint</span><input value={draft.checkpointId} onChange={(event) => setField('checkpointId', event.target.value)} /></label>
          <label><span>Mode</span>
            <select value={draft.mode} onChange={(event) => setField('mode', testMode(event.target.value))}>
              <option value="historical">Historical checkpoint</option>
              <option value="current">Current knowledge</option>
            </select>
          </label>
          <label><span>Target agent</span><input value={draft.targetAgent} onChange={(event) => setField('targetAgent', event.target.value)} /></label>
          <label className="continuity-task"><span>Task</span><textarea rows={1} value={draft.task} onChange={(event) => setField('task', event.target.value)} /></label>
        </div>
        <div className="continuity-actions">
          <button type="button" className="continuity-prepare" onClick={preparePrompt}>Prepare new-chat prompt</button>
          <button type="button" onClick={() => void copy('chat')}>Copy Chat ID</button>
          <button type="button" onClick={() => void copy('turn')}>Copy Turn ID</button>
          <button type="button" onClick={() => void copy('ax-session')}>Copy Ax Session ID</button>
          <button type="button" onClick={() => void copy('all')}>Copy All IDs</button>
          <button type="button" onClick={() => void copy('reference')}>Copy reference</button>
        </div>
        <div className="continuity-prompt">
          <label>
            <span>New-chat prompt</span>
            <textarea id="continuity-prompt-text" rows={8} value={prompt} onChange={(event) => { setPromptEdited(true); setPrompt(event.target.value); }} />
          </label>
          <button type="button" className="continuity-prompt-copy" aria-label="Copy prompt" onClick={() => void copyPrompt()}>Copy</button>
        </div>
        <p className="continuity-notice" role="status">{notice}</p>
      </div>
    </section>
  );
}
