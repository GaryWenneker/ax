import { useCallback, useEffect, useMemo, useState } from 'react';
import { createPortal } from 'react-dom';

import Codicon from './Codicon';
import { displayPath, pathBreadcrumbs } from '../pathDisplay';
import {
  browseWorkspace,
  fetchWorkspaceCurrent,
  mkdirWorkspace,
  streamWorkspaceInit,
  switchWorkspace,
  type BrowseEntry,
  type RecentProject,
} from '../workspaceApi';
import { notifyWorkspaceSwitched } from '../workspaceEvents';

interface Props {
  onClose: () => void;
  onSwitched?: () => void;
}

function AxProjectBadge({ compact }: { compact?: boolean }) {
  return (
    <span className="ax-project-badge" title="ax project — indexed knowledge graph">
      <Codicon name="symbol-structure" className="ax-project-badge-icon" />
      {!compact && <span>ax project</span>}
    </span>
  );
}

export default function ProjectBrowserModal({ onClose, onSwitched }: Props) {
  const [currentPath, setCurrentPath] = useState('');
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [browsePath, setBrowsePath] = useState('');
  const [browseInitialized, setBrowseInitialized] = useState(false);
  const [entries, setEntries] = useState<BrowseEntry[]>([]);
  const [parent, setParent] = useState<string | undefined>();
  const [filter, setFilter] = useState('');
  const [axOnly, setAxOnly] = useState(false);
  const [newName, setNewName] = useState('');
  const [busy, setBusy] = useState<string | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [pathDraft, setPathDraft] = useState('');
  const [editingPath, setEditingPath] = useState(false);
  const loadBrowse = useCallback(async (path: string) => {
    setErr(null);
    const data = await browseWorkspace(path);
    if (data.ok && data.path) {
      const shown = displayPath(data.path);
      setBrowsePath(shown);
      setPathDraft(shown);
      setParent(data.parent ? displayPath(data.parent) : undefined);
      setBrowseInitialized(data.initialized === true);
      setEntries(data.entries ?? []);
      setEditingPath(false);
    } else {
      setErr(data.error ?? 'Browse failed');
    }
  }, []);

  useEffect(() => {
    void (async () => {
      const data = await fetchWorkspaceCurrent();
      if (data.workspace?.path) {
        const shown = displayPath(data.workspace.path);
        setCurrentPath(shown);
        setBrowsePath(shown);
        setPathDraft(shown);
      }
      if (data.recent) setRecent(data.recent);
    })();
    // Do not autofocus the filter — on mobile that opens the keyboard and
    // steals attention from Recent / browse. Users tap the field when needed.
  }, []);

  useEffect(() => {
    if (browsePath) void loadBrowse(browsePath);
  }, [browsePath, loadBrowse]);

  const crumbs = useMemo(() => pathBreadcrumbs(browsePath), [browsePath]);

  function goToPath(raw: string) {
    const next = displayPath(raw.trim());
    if (!next || next === browsePath) {
      setEditingPath(false);
      setPathDraft(browsePath);
      return;
    }
    setBrowsePath(next);
  }

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    const prev = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.removeEventListener('keydown', onKey);
      document.body.style.overflow = prev;
    };
  }, [onClose]);

  async function doSwitch(path: string) {
    setBusy('switch');
    setErr(null);
    const res = await switchWorkspace(path);
    setBusy(null);
    if (res.ok && res.reloading) {
      window.location.reload();
      return;
    }
    if (res.needs_init) {
      setErr('Not an ax project yet — use Initialize below.');
      setBrowsePath(path);
      return;
    }
    if (!res.ok) {
      setErr(res.error ?? 'Switch failed');
      return;
    }
    onClose();
    notifyWorkspaceSwitched(res.path);
    onSwitched?.();
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
    await loadBrowse(res.path ?? browsePath);
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

  const axRecent = recent.filter((r) => r.initialized);
  const needle = filter.trim().toLowerCase();
  const visibleEntries = entries
    .filter((e) => {
      if (axOnly && !e.initialized) return false;
      if (!needle) return true;
      return e.name.toLowerCase().includes(needle);
    })
    .slice()
    .sort(
      (a, b) =>
        Number(b.initialized) - Number(a.initialized) || a.name.localeCompare(b.name),
    );
  const currentShown = displayPath(currentPath);

  return createPortal(
    <div className="project-browser-overlay" role="presentation" onMouseDown={onClose}>
      <div
        className="project-browser-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Browse projects"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="project-browser-header">
          <div>
            <h2>Open ax project</h2>
            <p className="project-browser-sub">
              Browse your disk for indexed projects — look for the{' '}
              <AxProjectBadge compact /> badge.
            </p>
          </div>
          <button type="button" className="project-browser-close btn btn-subtle" onClick={onClose} aria-label="Close">
            <Codicon name="close" />
          </button>
        </header>

        {axRecent.length > 0 && (
          <section className="project-browser-section project-browser-recent">
            <h3 className="project-browser-section-title">
              <Codicon name="history" />
              Recent ax projects
            </h3>
            <div className="project-browser-recent-grid">
              {axRecent.map((r) => (
                <button
                  key={r.path}
                  type="button"
                  className={`project-browser-recent-card${displayPath(r.path) === currentShown ? ' project-browser-recent-card--active' : ''}`}
                  onClick={() => void doSwitch(r.path)}
                  disabled={!!busy}
                  title={displayPath(r.path)}
                >
                  <span className="project-browser-recent-icon" aria-hidden="true">
                    <Codicon name="symbol-structure" />
                  </span>
                  <span className="project-browser-recent-label">{r.label}</span>
                  <AxProjectBadge compact />
                </button>
              ))}
            </div>
          </section>
        )}

        <section className="project-browser-section project-browser-picker">
          <h3 className="project-browser-section-title">
            <Codicon name="folder-opened" />
            Folder picker
          </h3>
          <div className="project-browser-toolbar">
            <div className="project-browser-path-row">
              {parent && (
                <button
                  type="button"
                  className="btn btn-subtle btn-sm"
                  onClick={() => setBrowsePath(parent)}
                  disabled={!!busy}
                  title="Parent folder"
                >
                  <Codicon name="arrow-up" />
                </button>
              )}
              {editingPath ? (
                <form
                  className="project-browser-path-edit"
                  onSubmit={(e) => {
                    e.preventDefault();
                    goToPath(pathDraft);
                  }}
                >
                  <input
                    className="settings-input project-browser-path-input mono"
                    value={pathDraft}
                    onChange={(e) => setPathDraft(e.target.value)}
                    disabled={!!busy}
                    autoFocus
                    spellCheck={false}
                    aria-label="Folder path"
                    onBlur={() => {
                      if (pathDraft.trim() === browsePath) setEditingPath(false);
                    }}
                  />
                  <button type="submit" className="btn btn-subtle btn-sm" disabled={!!busy}>
                    Go
                  </button>
                </form>
              ) : (
                <nav className="project-browser-crumbs" aria-label="Current path">
                  {crumbs.map((c, i) => (
                    <span key={c.path} className="project-browser-crumb-wrap">
                      {i > 0 && <span className="project-browser-crumb-sep" aria-hidden="true">/</span>}
                      <button
                        type="button"
                        className={`project-browser-crumb${i === crumbs.length - 1 ? ' project-browser-crumb--current' : ''}`}
                        onClick={() => (i === crumbs.length - 1 ? setEditingPath(true) : setBrowsePath(c.path))}
                        disabled={!!busy}
                        title={c.path}
                      >
                        {c.label}
                      </button>
                    </span>
                  ))}
                  <button
                    type="button"
                    className="btn btn-subtle btn-sm project-browser-path-edit-btn"
                    onClick={() => {
                      setPathDraft(browsePath);
                      setEditingPath(true);
                    }}
                    disabled={!!busy}
                    title="Edit path"
                    aria-label="Edit path"
                  >
                    <Codicon name="edit" />
                  </button>
                </nav>
              )}
            </div>
            <div className="project-browser-filters">
              <input
                type="search"
                className="settings-input project-browser-search"
                placeholder="Filter folders…"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                disabled={!!busy}
              />
              <label className="project-browser-ax-only">
                <input
                  type="checkbox"
                  checked={axOnly}
                  onChange={(e) => setAxOnly(e.target.checked)}
                  disabled={!!busy}
                />
                ax projects only
              </label>
            </div>
          </div>

          <div className="project-browser-list" role="list">
            {visibleEntries.length === 0 && (
              <p className="project-browser-empty">
                {err
                  ? err
                  : axOnly
                    ? 'No ax projects in this folder.'
                    : 'No subfolders here.'}
              </p>
            )}
            {visibleEntries.map((e) => (
              <div
                key={e.path}
                role="listitem"
                className={`project-browser-row${e.initialized ? ' project-browser-row--ax' : ''}${displayPath(e.path) === currentShown ? ' project-browser-row--current' : ''}`}
              >
                <button
                  type="button"
                  className="project-browser-row-main"
                  onClick={() => (e.initialized ? void doSwitch(e.path) : setBrowsePath(displayPath(e.path)))}
                  disabled={!!busy}
                  title={displayPath(e.path)}
                >
                  <span className={`project-browser-folder-icon${e.initialized ? ' project-browser-folder-icon--ax' : ''}`}>
                    <Codicon name={e.initialized ? 'symbol-structure' : 'folder'} />
                  </span>
                  <span className="project-browser-row-name">{e.name}</span>
                  {e.initialized && <AxProjectBadge />}
                </button>
                {!e.initialized && (
                  <button
                    type="button"
                    className="btn btn-subtle btn-sm project-browser-row-enter"
                    onClick={() => setBrowsePath(displayPath(e.path))}
                    disabled={!!busy}
                    title="Open folder"
                  >
                    <Codicon name="chevron-right" />
                  </button>
                )}
                {e.initialized && displayPath(e.path) !== currentShown && (
                  <button
                    type="button"
                    className="btn primary btn-sm"
                    onClick={() => void doSwitch(e.path)}
                    disabled={!!busy}
                  >
                    Switch
                  </button>
                )}
                {displayPath(e.path) === currentShown && (
                  <span className="project-browser-current-pill">current</span>
                )}
              </div>
            ))}
          </div>

          <div className="project-browser-footer-actions">
            <div className="project-browser-create">
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
            <div className="project-browser-open-actions">
              {(browseInitialized) ? (
                <button type="button" className="btn primary" onClick={() => void doSwitch(browsePath)} disabled={!!busy}>
                  Switch to this project
                </button>
              ) : (
                <>
                  <button type="button" className="btn btn-subtle" onClick={() => void doSwitch(browsePath)} disabled={!!busy}>
                    Open folder
                  </button>
                  <button type="button" className="btn btn-subtle" onClick={() => void doInit(browsePath)} disabled={!!busy}>
                    Initialize (ax init)
                  </button>
                </>
              )}
            </div>
          </div>
        </section>

        {err && <div className="project-browser-toast project-browser-toast--err">{err}</div>}
        {log.length > 0 && (
          <pre className="settings-log-body project-browser-log">{log.join('\n')}</pre>
        )}
      </div>
    </div>,
    document.body,
  );
}
