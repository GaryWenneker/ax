import { useEffect, useRef, useState, type ReactNode } from 'react';
import { fetchStats, fetchVersion } from '../api';
import Codicon from './Codicon';
import ProjectBrowserModal from './ProjectBrowserModal';
import { useUiContext } from '../context/UiContext';
import { WORKSPACE_SWITCHED } from '../workspaceEvents';
import type { Stats } from '../types';

function IconNodes() {
  return (
    <svg className="status-icon" width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <circle cx="4" cy="8" r="2" />
      <circle cx="12" cy="4" r="2" />
      <circle cx="12" cy="12" r="2" />
      <path d="M6 7.5L10 4.5M6 8.5L10 11.5" stroke="currentColor" strokeWidth="1" fill="none" />
    </svg>
  );
}

function IconLink() {
  return (
    <svg className="status-icon" width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M6.5 9.5L9.5 6.5M5 11a3 3 0 010-4.2l1.8-1.8a3 3 0 014.2 4.2l-.3.3M11 5a3 3 0 010 4.2l-1.8 1.8a3 3 0 01-4.2-4.2l.3-.3" stroke="currentColor" strokeWidth="1.2" fill="none" />
    </svg>
  );
}

function IconFile() {
  return (
    <svg className="status-icon" width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M4 2h5l3 3v9a1 1 0 01-1 1H4a1 1 0 01-1-1V3a1 1 0 011-1zm4 0v3h3" fill="none" stroke="currentColor" strokeWidth="1" />
    </svg>
  );
}

function IconCode() {
  return (
    <svg className="status-icon" width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M5 4L1 8l4 4M11 4l4 4-4 4" stroke="currentColor" strokeWidth="1.2" fill="none" />
    </svg>
  );
}

function IconShield() {
  return (
    <svg className="status-icon" width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M8 1l5 2v4c0 3.5-2.5 5.5-5 6-2.5-.5-5-2.5-5-6V3l5-2z" fill="none" stroke="currentColor" strokeWidth="1" />
    </svg>
  );
}

function IconWarn() {
  return (
    <svg className="status-icon" width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M8 1L1 14h14L8 1zm0 4v4M8 11v1" stroke="currentColor" strokeWidth="1.2" fill="none" />
    </svg>
  );
}

function IconClock() {
  return (
    <svg className="status-icon" width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="1" />
      <path d="M8 5v3l2 2" stroke="currentColor" strokeWidth="1" fill="none" />
    </svg>
  );
}

function IconDb() {
  return (
    <svg className="status-icon" width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <ellipse cx="8" cy="4" rx="5" ry="2" fill="none" stroke="currentColor" strokeWidth="1" />
      <path d="M3 4v4c0 1.1 2.2 2 5 2s5-.9 5-2V4M3 8v4c0 1.1 2.2 2 5 2s5-.9 5-2V8" fill="none" stroke="currentColor" strokeWidth="1" />
    </svg>
  );
}

function formatBytes(b: number) {
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${(b / (1024 * 1024)).toFixed(1)} MB`;
}

function formatRelative(ts: number) {
  if (!ts) return 'never';
  const diff = Date.now() - ts;
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 48) return `${hrs}h ago`;
  return new Date(ts).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

function formatFullDate(ts: number) {
  if (!ts) return 'Never indexed';
  return new Date(ts).toLocaleString();
}

function navigateHash(page: string, params?: Record<string, string>) {
  const sp = new URLSearchParams(params);
  const qs = sp.toString();
  const next = qs ? `${page}?${qs}` : page;
  if (window.location.hash.replace(/^#/, '') !== next) {
    window.location.hash = next;
  }
}

function StatusChip({
  id,
  title,
  openPanel,
  onTogglePanel,
  onNavigate,
  className = '',
  children,
  panel,
}: {
  id: string;
  title: string;
  openPanel: string | null;
  onTogglePanel: (id: string | null) => void;
  onNavigate?: () => void;
  className?: string;
  children: ReactNode;
  panel?: ReactNode;
}) {
  const isOpen = openPanel === id;

  function handleClick() {
    if (panel) {
      onTogglePanel(isOpen ? null : id);
      return;
    }
    onNavigate?.();
  }

  return (
    <span className={`status-chip-wrap${isOpen ? ' status-chip-wrap--open' : ''}`}>
      <button
        type="button"
        className={`status-item status-item--clickable${className ? ` ${className}` : ''}${isOpen ? ' status-item--active' : ''}`}
        title={title}
        aria-expanded={panel ? isOpen : undefined}
        onClick={handleClick}
      >
        {children}
      </button>
      {panel && isOpen && (
        <div className="status-panel" role="region" aria-label={title}>
          {panel}
        </div>
      )}
    </span>
  );
}

function PanelLink({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button type="button" className="status-panel-link" onClick={onClick}>
      {label}
    </button>
  );
}

export default function StatusBar() {
  const { pageContext } = useUiContext();
  const [stats, setStats] = useState<Stats | null>(null);
  const [version, setVersion] = useState('');
  const [openPanel, setOpenPanel] = useState<string | null>(null);
  const [projectModalOpen, setProjectModalOpen] = useState(false);
  const barRef = useRef<HTMLElement>(null);

  function refresh() {
    fetchStats().then(setStats).catch(() => {});
    fetchVersion().then((v) => setVersion(v.version)).catch(() => {});
  }

  useEffect(() => {
    refresh();
    const id = window.setInterval(refresh, 30_000);
    const onFocus = () => refresh();
    const onWorkspaceSwitched = () => refresh();
    window.addEventListener('focus', onFocus);
    window.addEventListener(WORKSPACE_SWITCHED, onWorkspaceSwitched);
    return () => {
      clearInterval(id);
      window.removeEventListener('focus', onFocus);
      window.removeEventListener(WORKSPACE_SWITCHED, onWorkspaceSwitched);
    };
  }, []);

  useEffect(() => {
    function onDocClick(e: MouseEvent) {
      if (!barRef.current?.contains(e.target as Node)) {
        setOpenPanel(null);
      }
    }
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, []);

  function nav(page: string, params?: Record<string, string>) {
    setOpenPanel(null);
    navigateHash(page, params);
  }

  const center = pageContext.detail
    ? `${pageContext.view} · ${pageContext.detail}`
    : pageContext.view;

  const unresolvedCount = stats?.unresolved_ref_count ?? 0;

  return (
    <footer className="statusbar" aria-label="Status" ref={barRef}>
      <div className="statusbar-left">
        <StatusChip
          id="nodes"
          title="Indexed symbols — open Nodes"
          openPanel={openPanel}
          onTogglePanel={setOpenPanel}
          onNavigate={() => nav('nodes')}
        >
          <IconNodes />
          <span className="status-val">{stats ? stats.node_count.toLocaleString() : '—'}</span>
          <span className="status-lbl">nodes</span>
        </StatusChip>
        <span className="status-sep" aria-hidden="true">|</span>
        <StatusChip
          id="edges"
          title="Graph edges — open Stats"
          openPanel={openPanel}
          onTogglePanel={setOpenPanel}
          onNavigate={() => nav('stats')}
          panel={
            stats ? (
              <>
                <div className="status-panel-title">Graph edges</div>
                <p className="status-panel-text">
                  {stats.edge_count.toLocaleString()} relationships between indexed symbols (calls, contains, imports, …).
                </p>
                <PanelLink label="Open Stats →" onClick={() => nav('stats')} />
              </>
            ) : null
          }
        >
          <IconLink />
          <span className="status-val">{stats ? stats.edge_count.toLocaleString() : '—'}</span>
          <span className="status-lbl">edges</span>
        </StatusChip>
        <span className="status-sep" aria-hidden="true">|</span>
        <StatusChip
          id="files"
          title="Indexed files — open Files explorer"
          openPanel={openPanel}
          onTogglePanel={setOpenPanel}
          onNavigate={() => nav('files')}
        >
          <IconFile />
          <span className="status-val">{stats ? stats.file_count.toLocaleString() : '—'}</span>
          <span className="status-lbl">files</span>
        </StatusChip>
        <span className="status-sep" aria-hidden="true">|</span>
        <StatusChip
          id="langs"
          title="Languages in index"
          openPanel={openPanel}
          onTogglePanel={setOpenPanel}
          panel={
            stats ? (
              <>
                <div className="status-panel-title">Languages ({stats.languages.length})</div>
                <ul className="status-panel-list">
                  {stats.languages.slice(0, 8).map((l) => (
                    <li key={l.language}>
                      <span className="mono">{l.language}</span>
                      <span className="status-panel-list-val">{l.count.toLocaleString()}</span>
                    </li>
                  ))}
                </ul>
                {stats.languages.length > 8 && (
                  <p className="status-panel-muted">+{stats.languages.length - 8} more in Stats</p>
                )}
                <PanelLink label="Open Stats →" onClick={() => nav('stats')} />
              </>
            ) : null
          }
        >
          <IconCode />
          <span className="status-val">{stats ? stats.languages.length : '—'}</span>
          <span className="status-lbl">langs</span>
        </StatusChip>
      </div>

      <button
        type="button"
        className="statusbar-center statusbar-center--clickable"
        title={`${center} — open Stats`}
        onClick={() => nav('stats')}
      >
        {center}
      </button>

      <div className="statusbar-right">
        {unresolvedCount > 0 && (
          <StatusChip
            id="unresolved"
            title="Unresolved symbol references — open list"
            openPanel={openPanel}
            onTogglePanel={setOpenPanel}
            onNavigate={() => nav('unresolved')}
            className="status-warn"
          >
            <IconWarn />
            <span className="status-val">{unresolvedCount.toLocaleString()}</span>
            <span className="status-lbl">unresolved</span>
          </StatusChip>
        )}
        <StatusChip
          id="rules"
          title="Policy rules"
          openPanel={openPanel}
          onTogglePanel={setOpenPanel}
          onNavigate={() => nav('policy-rules')}
        >
          <IconShield />
          <span className="status-val">{stats ? stats.policy_rules_count : '—'}</span>
          <span className="status-lbl">rules</span>
        </StatusChip>
        <StatusChip
          id="skills"
          title="Policy skills"
          openPanel={openPanel}
          onTogglePanel={setOpenPanel}
          onNavigate={() => nav('policy-skills')}
        >
          <IconShield />
          <span className="status-val">{stats ? stats.policy_skills_count : '—'}</span>
          <span className="status-lbl">skills</span>
        </StatusChip>
        <StatusChip
          id="indexed"
          title="Last index run"
          openPanel={openPanel}
          onTogglePanel={setOpenPanel}
          panel={
            stats ? (
              <>
                <div className="status-panel-title">Last indexed</div>
                <p className="status-panel-text">{formatFullDate(stats.last_indexed_at)}</p>
                <PanelLink label="Open Stats →" onClick={() => nav('stats')} />
              </>
            ) : null
          }
        >
          <IconClock />
          <span className="status-lbl">{stats ? formatRelative(stats.last_indexed_at) : '—'}</span>
        </StatusChip>
        <StatusChip
          id="db"
          title="Database size"
          openPanel={openPanel}
          onTogglePanel={setOpenPanel}
          panel={
            stats ? (
              <>
                <div className="status-panel-title">Index database</div>
                <ul className="status-panel-list">
                  <li>
                    <span>Size</span>
                    <span className="status-panel-list-val">{formatBytes(stats.db_size_bytes)}</span>
                  </li>
                  <li>
                    <span>Files</span>
                    <span className="status-panel-list-val">{stats.file_count.toLocaleString()}</span>
                  </li>
                  <li>
                    <span>Nodes</span>
                    <span className="status-panel-list-val">{stats.node_count.toLocaleString()}</span>
                  </li>
                </ul>
                <PanelLink label="Open Stats →" onClick={() => nav('stats')} />
              </>
            ) : null
          }
        >
          <IconDb />
          <span className="status-lbl">{stats ? formatBytes(stats.db_size_bytes) : '—'}</span>
        </StatusChip>
        {stats?.project_name && (
          <button
            type="button"
            className="status-item status-item--clickable status-project"
            title={`${stats.project_name} — browse for ax projects`}
            onClick={() => setProjectModalOpen(true)}
          >
            <Codicon name="symbol-structure" className="status-project-icon" />
            <span className="status-lbl">{stats.project_name}</span>
          </button>
        )}
        {projectModalOpen && (
          <ProjectBrowserModal
            onClose={() => setProjectModalOpen(false)}
            onSwitched={() => setProjectModalOpen(false)}
          />
        )}
        {stats?.readonly && (
          <span className="status-item status-readonly" title="Read-only mode">
            <span className="status-lbl">read-only</span>
          </span>
        )}
        {version && (
          <span className="status-item" title="ax version">
            <span className="status-lbl">v{version}</span>
          </span>
        )}
      </div>
    </footer>
  );
}
