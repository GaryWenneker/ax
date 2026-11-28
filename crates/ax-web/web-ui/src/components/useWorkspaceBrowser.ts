import { useCallback, useEffect, useRef, useState } from 'react';

import { createLatestGate } from '../lib/latestLoad';
import { browseOutcome, EMPTY_BROWSE_VIEW, type BrowseResponse, type BrowseView } from '../lib/projectBrowse';
import { mkdirOutcome, switchOutcome } from '../lib/workspaceSwitch';
import { browseWorkspace, mkdirWorkspace, streamWorkspaceInit, switchWorkspace } from '../workspaceApi';
import { notifyWorkspaceSwitched } from '../workspaceEvents';

interface Options {
  /** Called after the server confirmed a switch (the switch event is already sent). */
  onSwitched?: (path?: string) => void;
  /** Called after each browse that is still the latest, with the path to put in a path field. */
  onBrowsed?: (draft: string, ok: boolean) => void;
}

/**
 * Browse, switch, create-folder and init state shared by the project browser
 * modal and the compact workspace picker. Every navigation goes through
 * `loadBrowse`; nothing else starts a browse request.
 */
export function useWorkspaceBrowser({ onSwitched, onBrowsed }: Options = {}) {
  const [view, setView] = useState<BrowseView>(EMPTY_BROWSE_VIEW);
  const viewRef = useRef(view);
  viewRef.current = view;
  const gate = useRef(createLatestGate()).current;
  const [busy, setBusy] = useState<string | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [newName, setNewName] = useState('');
  const callbacks = useRef({ onSwitched, onBrowsed });
  useEffect(() => {
    callbacks.current = { onSwitched, onBrowsed };
  }, [onSwitched, onBrowsed]);

  /** Browse `path`. `notice` is shown instead of clearing the message when the browse succeeds. */
  const loadBrowse = useCallback(
    async (path: string, notice?: string) => {
      const ticket = gate.begin();
      setErr(null);
      let data: BrowseResponse;
      try {
        data = await browseWorkspace(path);
      } catch (e) {
        data = { ok: false, error: e instanceof Error ? e.message : String(e) };
      }
      const out = browseOutcome(viewRef.current, data, gate.isLatest(ticket));
      if (!out) return;
      setView(out.view);
      setErr(out.error ?? notice ?? null);
      callbacks.current.onBrowsed?.(out.draft, out.error === null);
    },
    [gate],
  );

  const doSwitch = useCallback(
    async (path: string) => {
      setBusy('switch');
      setErr(null);
      const res = await switchWorkspace(path);
      setBusy(null);
      const outcome = switchOutcome(res, path);
      switch (outcome.kind) {
        case 'reload':
          setLog(['Switching project — reloading…']);
          window.location.reload();
          return;
        case 'needs_init':
          await loadBrowse(outcome.path, outcome.message);
          return;
        case 'error':
          setErr(outcome.message);
          return;
        case 'switched':
          notifyWorkspaceSwitched(outcome.path);
          callbacks.current.onSwitched?.(outcome.path);
      }
    },
    [loadBrowse],
  );

  const doMkdir = useCallback(async () => {
    const name = newName.trim();
    const shown = viewRef.current.path;
    if (!name || !shown) return;
    setBusy('mkdir');
    setErr(null);
    const res = await mkdirWorkspace(shown, name);
    setBusy(null);
    const out = mkdirOutcome(res, shown);
    if (out.error) {
      setErr(out.error);
      return;
    }
    setNewName('');
    if (out.browse) await loadBrowse(out.browse);
  }, [newName, loadBrowse]);

  const doInit = useCallback(
    async (path: string) => {
      setBusy('init');
      setLog([]);
      setErr(null);
      await streamWorkspaceInit(path, (ev) => {
        if (ev.type === 'line') setLog((l) => [...l, ev.text]);
        if (ev.type === 'done') {
          setBusy(null);
          if (!ev.ok) setErr(ev.error ?? 'Init failed');
          else if (ev.initialized) void doSwitch(path);
        }
      });
    },
    [doSwitch],
  );

  return { view, busy, log, err, setErr, newName, setNewName, loadBrowse, doSwitch, doMkdir, doInit };
}
