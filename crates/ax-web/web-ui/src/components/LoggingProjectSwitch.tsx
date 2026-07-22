import { useCallback, useEffect, useRef, useState } from 'react';

import Codicon from './Codicon';
import {
  fetchWorkspaceCurrent,
  fetchWorkspaceRecent,
  switchWorkspace,
  type RecentProject,
} from '../workspaceApi';
import { notifyWorkspaceSwitched } from '../workspaceEvents';

type Props = {
  /** Highlight the active project path when known. */
  currentPath?: string;
  currentLabel?: string;
  /** `banner` = dropdown in logging banner; `inline` = always-visible list (status panel). */
  variant?: 'banner' | 'inline';
  onSwitched?: () => void;
};

/**
 * Quick switch between recent projects while viewing MCP Logging.
 * Switching remounts Logging via WORKSPACE_SWITCHED (follows that project's .ax/mcp-verbose.log).
 */
export default function LoggingProjectSwitch({
  currentPath = '',
  currentLabel = '',
  variant = 'banner',
  onSwitched,
}: Props) {
  const [open, setOpen] = useState(variant === 'inline');
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [label, setLabel] = useState(currentLabel);
  const [path, setPath] = useState(currentPath);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const isInline = variant === 'inline';

  const refresh = useCallback(async () => {
    const [cur, rec] = await Promise.all([fetchWorkspaceCurrent(), fetchWorkspaceRecent()]);
    if (cur.workspace) {
      setPath(cur.workspace.path);
      setLabel(cur.workspace.label);
    }
    const list = rec.ok ? rec.recent : cur.recent ?? [];
    setRecent(list);
  }, []);

  useEffect(() => {
    if (currentLabel) setLabel(currentLabel);
    if (currentPath) setPath(currentPath);
  }, [currentLabel, currentPath]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (isInline || !open) return;
    void refresh();
    function onDoc(e: MouseEvent) {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener('mousedown', onDoc);
    return () => document.removeEventListener('mousedown', onDoc);
  }, [open, refresh, isInline]);

  async function doSwitch(nextPath: string) {
    if (!nextPath || nextPath === path) {
      if (!isInline) setOpen(false);
      return;
    }
    setBusy(true);
    setErr(null);
    const res = await switchWorkspace(nextPath);
    setBusy(false);
    if (res.ok && res.switched) {
      if (!isInline) setOpen(false);
      notifyWorkspaceSwitched(res.path);
      onSwitched?.();
      return;
    }
    setErr(res.error ?? 'Switch failed');
  }

  const list = (
    <>
      {err && <p className="logging-proj-switch-err">{err}</p>}
      {recent.length === 0 ? (
        <p className="logging-proj-switch-empty">No recent projects yet.</p>
      ) : (
        <ul className="logging-proj-switch-list">
          {recent.map((p) => {
            const active = p.path === path;
            return (
              <li key={p.path}>
                <button
                  type="button"
                  role="option"
                  aria-selected={active}
                  className={`logging-proj-switch-item${active ? ' logging-proj-switch-item--active' : ''}`}
                  title={p.path}
                  disabled={busy}
                  onClick={() => void doSwitch(p.path)}
                >
                  <span className="logging-proj-switch-item-name">{p.label}</span>
                  <span className="logging-proj-switch-item-path">{p.path}</span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </>
  );

  if (isInline) {
    return (
      <div className="logging-proj-switch logging-proj-switch--inline" role="listbox" aria-label="Recent projects">
        {list}
      </div>
    );
  }

  return (
    <div
      ref={wrapRef}
      className={`logging-proj-switch logging-proj-switch--banner${open ? ' logging-proj-switch--open' : ''}`}
    >
      <button
        type="button"
        className="btn btn-compact logging-proj-switch-btn"
        title="Switch project log"
        aria-expanded={open}
        aria-haspopup="listbox"
        disabled={busy}
        onClick={() => setOpen((v) => !v)}
      >
        <Codicon name="folder" />
        <span className="logging-proj-switch-label">{label || 'Project'}</span>
        <Codicon name={open ? 'chevron-up' : 'chevron-down'} />
      </button>

      {open && (
        <div className="logging-proj-switch-menu" role="listbox" aria-label="Recent projects">
          <div className="logging-proj-switch-menu-title">Switch project log</div>
          {list}
        </div>
      )}
    </div>
  );
}
