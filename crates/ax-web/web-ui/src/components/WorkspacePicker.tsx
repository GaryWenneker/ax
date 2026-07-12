import { useCallback, useEffect, useRef, useState } from 'react';

import Codicon from './Codicon';
import {
  browseWorkspace,
  fetchWorkspaceCurrent,
  mkdirWorkspace,
  streamWorkspaceInit,
  switchWorkspace,
  type BrowseEntry,
  type RecentProject,
} from '../workspaceApi';

interface Props {
  compact?: boolean;
  onSwitched?: () => void;
}

export default function WorkspacePicker({ compact, onSwitched }: Props) {
  const [open, setOpen] = useState(false);
  const [currentPath, setCurrentPath] = useState('');
  const [currentLabel, setCurrentLabel] = useState('');
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [browsePath, setBrowsePath] = useState('');
  const [entries, setEntries] = useState<BrowseEntry[]>([]);
  const [parent, setParent] = useState<string | undefined>();
  const [newName, setNewName] = useState('');
  const [busy, setBusy] = useState<string | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const wrapRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    const data = await fetchWorkspaceCurrent();
    if (data.workspace) {
      setCurrentPath(data.workspace.path);
      setCurrentLabel(data.workspace.label);
      setBrowsePath(data.workspace.path);
    }
    if (data.recent) setRecent(data.recent);
  }, []);

  const loadBrowse = useCallback(async (path: string) => {
    const data = await browseWorkspace(path);
    if (data.ok && data.path) {
      setBrowsePath(data.path);
      setParent(data.parent);
      setEntries(data.entries ?? []);
    } else {
      setErr(data.error ?? 'Browse failed');
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (open && browsePath) void loadBrowse(browsePath);
  }, [open, browsePath, loadBrowse]);

  useEffect(() => {
    function onDoc(e: MouseEvent) {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener('mousedown', onDoc);
    return () => document.removeEventListener('mousedown', onDoc);
  }, []);

  async function doSwitch(path: string) {
    setBusy('switch');
    setErr(null);
    const res = await switchWorkspace(path);
    setBusy(null);
    if (res.ok && res.switched) {
      setLog(['Switched project — refreshing data…']);
      setOpen(false);
      onSwitched?.();
      void refresh();
      return;
    }
    if (res.ok && res.reloading) {
      setLog(['Switching project — reloading…']);
      window.location.reload();
      return;
    }
    if (res.needs_init) {
      setErr('Project not initialized — use Initialize below.');
      setBrowsePath(path);
      return;
    }
    if (!res.ok) {
      setErr(res.error ?? 'Switch failed');
      return;
    }
    setOpen(false);
    onSwitched?.();
    void refresh();
  }

  async function doMkdir() {
    if (!newName.trim() || !browsePath) return;
    setBusy('mkdir');
    setErr(null);
    const res = await mkdirWorkspace(browsePath, newName.trim());
    setBusy(null);
    if (!res.ok) {
      setErr(res.error ?? 'Create failed');
      return;
    }
    setNewName('');
    if (res.path) await loadBrowse(res.path);
    else await loadBrowse(browsePath);
  }

  async function doInit(path: string) {
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
  }

  return (
    <div className={`workspace-picker${compact ? ' workspace-picker--compact' : ''}`} ref={wrapRef}>
      <button
        type="button"
        className="workspace-picker-trigger"
        onClick={() => setOpen(!open)}
        title={currentPath || 'Project'}
        aria-expanded={open}
      >
        <Codicon name="folder" />
        <span className="workspace-picker-label">{currentLabel || 'Project'}</span>
        <Codicon name="chevron-down" />
      </button>

      {open && (
        <div className="workspace-picker-panel" role="dialog" aria-label="Switch project">
          <div className="workspace-picker-section">
            <div className="workspace-picker-heading">Recent</div>
            {recent.length === 0 && <p className="workspace-picker-muted">No recent projects</p>}
            {recent.map((r) => (
              <button
                key={r.path}
                type="button"
                className={`workspace-picker-item${r.path === currentPath ? ' active' : ''}`}
                onClick={() => void doSwitch(r.path)}
                disabled={!!busy}
              >
                <span>{r.label}</span>
                <span className="workspace-picker-meta">{r.initialized ? 'indexed' : 'new'}</span>
              </button>
            ))}
          </div>

          <div className="workspace-picker-section">
            <div className="workspace-picker-heading">Browse</div>
            <div className="workspace-picker-path mono">{browsePath}</div>
            {parent && (
              <button type="button" className="btn btn-subtle btn-sm" onClick={() => void loadBrowse(parent)} disabled={!!busy}>
                ↑ Up
              </button>
            )}
            <div className="workspace-picker-list">
              {entries.map((e) => (
                <button
                  key={e.path}
                  type="button"
                  className="workspace-picker-item"
                  onClick={() => void loadBrowse(e.path)}
                  disabled={!!busy}
                >
                  <Codicon name="folder" />
                  <span>{e.name}</span>
                  {e.initialized && <span className="workspace-picker-meta">ax</span>}
                </button>
              ))}
            </div>
            <div className="workspace-picker-actions">
              <input
                className="settings-input"
                placeholder="New folder name"
                value={newName}
                onChange={(ev) => setNewName(ev.target.value)}
                disabled={!!busy}
              />
              <button type="button" className="btn btn-subtle" onClick={() => void doMkdir()} disabled={!!busy || !newName.trim()}>
                Create
              </button>
            </div>
            <div className="workspace-picker-actions">
              <button type="button" className="btn primary" onClick={() => void doSwitch(browsePath)} disabled={!!busy}>
                Open
              </button>
              <button type="button" className="btn btn-subtle" onClick={() => void doInit(browsePath)} disabled={!!busy}>
                Initialize (ax init)
              </button>
            </div>
          </div>

          {err && <div className="settings-toast settings-toast--err">{err}</div>}
          {log.length > 0 && (
            <pre className="settings-log-body workspace-picker-log">{log.join('\n')}</pre>
          )}
        </div>
      )}
    </div>
  );
}
