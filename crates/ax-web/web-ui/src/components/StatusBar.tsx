import { useEffect, useState } from 'react';
import { fetchStats, fetchVersion } from '../api';
import { useUiContext } from '../context/UiContext';
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

export default function StatusBar() {
  const { pageContext } = useUiContext();
  const [stats, setStats] = useState<Stats | null>(null);
  const [version, setVersion] = useState('');

  function refresh() {
    fetchStats().then(setStats).catch(() => {});
    fetchVersion().then((v) => setVersion(v.version)).catch(() => {});
  }

  useEffect(() => {
    refresh();
    const id = window.setInterval(refresh, 30_000);
    const onFocus = () => refresh();
    window.addEventListener('focus', onFocus);
    return () => {
      clearInterval(id);
      window.removeEventListener('focus', onFocus);
    };
  }, []);

  const center = pageContext.detail
    ? `${pageContext.view} · ${pageContext.detail}`
    : pageContext.view;

  return (
    <footer className="statusbar" aria-label="Status">
      <div className="statusbar-left">
        <span className="status-item" title="Indexed nodes">
          <IconNodes />
          <span className="status-val">{stats ? stats.node_count.toLocaleString() : '—'}</span>
          <span className="status-lbl">nodes</span>
        </span>
        <span className="status-sep" aria-hidden="true">|</span>
        <span className="status-item" title="Graph edges">
          <IconLink />
          <span className="status-val">{stats ? stats.edge_count.toLocaleString() : '—'}</span>
          <span className="status-lbl">edges</span>
        </span>
        <span className="status-sep" aria-hidden="true">|</span>
        <span className="status-item" title="Indexed files">
          <IconFile />
          <span className="status-val">{stats ? stats.file_count.toLocaleString() : '—'}</span>
          <span className="status-lbl">files</span>
        </span>
        <span className="status-sep" aria-hidden="true">|</span>
        <span className="status-item" title="Languages in index">
          <IconCode />
          <span className="status-val">{stats ? stats.languages.length : '—'}</span>
          <span className="status-lbl">langs</span>
        </span>
      </div>

      <div className="statusbar-center" title={center}>
        {center}
      </div>

      <div className="statusbar-right">
        {stats && stats.unresolved_ref_count != null && stats.unresolved_ref_count > 0 && (
          <span className="status-item status-warn" title="Unresolved references">
            <IconWarn />
            <span className="status-val">{stats.unresolved_ref_count}</span>
            <span className="status-lbl">unresolved</span>
          </span>
        )}
        <span className="status-item" title="Policy rules">
          <IconShield />
          <span className="status-val">{stats ? stats.policy_rules_count : '—'}</span>
          <span className="status-lbl">rules</span>
        </span>
        <span className="status-item" title="Policy skills">
          <IconShield />
          <span className="status-val">{stats ? stats.policy_skills_count : '—'}</span>
          <span className="status-lbl">skills</span>
        </span>
        <span className="status-item" title="Last index run">
          <IconClock />
          <span className="status-lbl">{stats ? formatRelative(stats.last_indexed_at) : '—'}</span>
        </span>
        <span className="status-item" title="Database size">
          <IconDb />
          <span className="status-lbl">{stats ? formatBytes(stats.db_size_bytes) : '—'}</span>
        </span>
        {stats?.project_name && (
          <span className="status-item status-project" title="Project">
            <span className="status-lbl">{stats.project_name}</span>
          </span>
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
