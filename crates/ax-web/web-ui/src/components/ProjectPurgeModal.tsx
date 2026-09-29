import { useEffect, useState } from 'react';

import ModalShell from './ModalShell';
import {
  fetchPurgePlan,
  purgeWorkspace,
  switchWorkspace,
  type PurgeGroup,
  type RecentProject,
} from '../workspaceApi';
import { notifyWorkspaceSwitched } from '../workspaceEvents';

function formatBytes(n: number): string {
  if (n <= 0) return '';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export default function ProjectPurgeModal({
  project,
  recent,
  onClose,
  onDone,
}: {
  project: RecentProject;
  recent: RecentProject[];
  onClose: () => void;
  onDone: () => void;
}) {
  const [groups, setGroups] = useState<PurgeGroup[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [note, setNote] = useState('');
  const [current, setCurrent] = useState(false);
  const [missing, setMissing] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void fetchPurgePlan(project.path).then((plan) => {
      if (cancelled) return;
      setLoading(false);
      if (!plan.ok || !plan.groups) {
        setLoadError(plan.error || 'Could not list ax data for this project');
        return;
      }
      setGroups(plan.groups);
      setNote(plan.note || '');
      setCurrent(Boolean(plan.current));
      setMissing(Boolean(plan.missing));
      setSelected(new Set(plan.groups.filter((g) => g.defaultOn && !g.empty).map((g) => g.id)));
    });
    return () => {
      cancelled = true;
    };
  }, [project.path]);

  function toggle(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function confirm() {
    if (selected.size === 0 || busy) return;
    setBusy(true);
    setError(null);
    const res = await purgeWorkspace(project.path, [...selected]);
    setBusy(false);
    if (res.disconnected) {
      const next = recent.find((p) => p.path !== project.path && p.initialized);
      if (next) {
        const switched = await switchWorkspace(next.path);
        if (switched.ok && switched.switched && switched.path) {
          notifyWorkspaceSwitched(switched.path);
        }
      }
    }
    if (!res.ok) {
      const detail = res.errors?.filter(Boolean).join('\n');
      setError(detail || res.error || 'Remove failed');
      return;
    }
    onDone();
  }

  const selectedBytes = groups
    .filter((g) => selected.has(g.id))
    .reduce((sum, g) => sum + g.bytes, 0);

  return (
    <ModalShell
      title={missing ? 'Folder already gone' : 'Remove ax data'}
      subtitle={project.label}
      onClose={onClose}
      ariaLabel={missing ? `Remove leftover entry for ${project.label}` : `Remove ax data for ${project.label}`}
      footer={
        <>
          <button type="button" className="btn btn-subtle" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button
            type="button"
            className="btn danger"
            onClick={() => void confirm()}
            disabled={busy || loading || selected.size === 0}
          >
            {busy ? 'Removing…' : 'Remove selected'}
          </button>
        </>
      }
    >
      <p className="project-purge-note">
        {note ||
          (missing
            ? 'This folder is already gone. Nothing on disk will be deleted.'
            : 'The repository source stays on disk.')}
      </p>
      <p className="project-purge-path" title={project.path}>
        {project.path}
      </p>
      {current && selected.has('database') && (
        <p className="project-purge-warn">
          This project is open. Its database connection will be closed. If another recent project
          exists, Command Center switches to it.
        </p>
      )}
      {loadError && <p className="project-purge-error">{loadError}</p>}
      {loading && <p className="project-purge-note">Checking what ax stored…</p>}
      <ul className="project-purge-list">
        {groups.map((g) => {
          const checked = selected.has(g.id);
          const disabled = g.empty;
          return (
            <li key={g.id}>
              <label className={`project-purge-item${disabled ? ' project-purge-item--empty' : ''}`}>
                <input
                  type="checkbox"
                  checked={checked && !disabled}
                  disabled={disabled || busy}
                  onChange={() => toggle(g.id)}
                />
                <span className="project-purge-item-body">
                  <span className="project-purge-item-title">
                    {g.label}
                    {g.bytes > 0 && <span className="project-purge-size">{formatBytes(g.bytes)}</span>}
                    {g.fileCount > 0 && !g.empty && (
                      <span className="project-purge-count">
                        {g.fileCount} {g.fileCount === 1 ? 'item' : 'items'}
                      </span>
                    )}
                  </span>
                  <span className="project-purge-item-detail">
                    {disabled ? 'Nothing stored.' : g.detail}
                  </span>
                </span>
              </label>
            </li>
          );
        })}
      </ul>
      {selected.size > 0 && (
        <p className="project-purge-summary">
          {selected.size} selected
          {selectedBytes > 0 ? ` · ${formatBytes(selectedBytes)}` : ''}
          {selected.has('agents') ? ' · includes .agents' : ''}
        </p>
      )}
      {error && <p className="project-purge-error">{error}</p>}
    </ModalShell>
  );
}
