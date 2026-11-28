import { useCallback, useEffect, useRef, useState } from 'react';

import Codicon from './Codicon';
import { fetchWorkspaceCurrent, type RecentProject } from '../workspaceApi';
import { useWorkspaceBrowser } from './useWorkspaceBrowser';

interface Props {
  compact?: boolean;
  onSwitched?: () => void;
}

export default function WorkspacePicker({ compact, onSwitched }: Props) {
  const [open, setOpen] = useState(false);
  const [currentPath, setCurrentPath] = useState('');
  const [currentLabel, setCurrentLabel] = useState('');
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const wrapRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    const data = await fetchWorkspaceCurrent();
    if (data.workspace) {
      setCurrentPath(data.workspace.path);
      setCurrentLabel(data.workspace.label);
    }
    if (data.recent) setRecent(data.recent);
  }, []);

  const afterSwitch = useCallback(() => {
    setOpen(false);
    onSwitched?.();
    void refresh();
  }, [onSwitched, refresh]);

  const { view, busy, log, err, newName, setNewName, loadBrowse, doSwitch, doMkdir, doInit } = useWorkspaceBrowser({
    onSwitched: afterSwitch,
  });
  const { path: browsePath, parent, entries } = view;

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Opening the panel shows the current project once; every later move is one
  // loadBrowse call from the button that was clicked.
  useEffect(() => {
    if (open && currentPath) void loadBrowse(currentPath);
  }, [open, currentPath, loadBrowse]);

  useEffect(() => {
    function onDoc(e: MouseEvent) {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener('mousedown', onDoc);
    return () => document.removeEventListener('mousedown', onDoc);
  }, []);

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
