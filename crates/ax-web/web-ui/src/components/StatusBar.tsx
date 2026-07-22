import { useEffect, useRef, useState, type ReactNode } from 'react';
import { fetchStats, fetchVersion } from '../api';
import { useMcpTraceStatus } from '../hooks/useMcpTraceStatus';
import Codicon from './Codicon';
import LoggingProjectSwitch from './LoggingProjectSwitch';
import ProjectBrowserModal from './ProjectBrowserModal';
import {
  emptyQualitySnapshot,
  fetchMcpQuality,
  gradeTone,
  openMcpQualitySlideout,
  type QualitySnapshot,
} from '../lib/mcpQuality';
import { useUiContext } from '../context/UiContext';
import {
  emptyMcpTraceStats,
  MCP_TRACE_STATS,
  publishMcpTraceFilter,
  type McpTraceStats,
} from '../lib/mcpTraceEvents';
import type { TraceKind } from '../lib/mcpTrace';
import { navigateRoute, pageFromNavId } from '../lib/routes';
import { currentThemeAccent, THEME_CHANGED } from '../lib/themes';
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

function IconLog() {
  return (
    <svg className="status-icon" width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path
        d="M3 2.5h10v11H3zM5 5h6M5 7.5h6M5 10h4"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.1"
      />
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

function navigateTo(page: string, params?: Record<string, string>) {
  const mapped = pageFromNavId(page);
  if (!mapped) return;
  navigateRoute({
    page: mapped,
    ruleId: params?.id ?? null,
    skillName: params?.name ?? null,
    kind: params?.kind ?? null,
    sonarTab: params?.tab === 'setup' ? 'setup' : 'dashboard',
  });
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
  const isLogging = pageContext.view === 'Logging';
  const [stats, setStats] = useState<Stats | null>(null);
  const [logStats, setLogStats] = useState<McpTraceStats>(emptyMcpTraceStats);
  const [quality, setQuality] = useState<QualitySnapshot>(emptyQualitySnapshot);
  const [version, setVersion] = useState('');
  const [openPanel, setOpenPanel] = useState<string | null>(null);
  const [projectModalOpen, setProjectModalOpen] = useState(false);
  const [accent, setAccent] = useState(() => currentThemeAccent());
  const barRef = useRef<HTMLElement>(null);
  const mcpTrace = useMcpTraceStatus();
  const offline = isLogging && !logStats.live;

  function refresh() {
    fetchStats().then(setStats).catch(() => {});
    fetchVersion().then((v) => setVersion(v.version)).catch(() => {});
    fetchMcpQuality().then(setQuality).catch(() => {});
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
    function onTheme(ev: Event) {
      const detail = (ev as CustomEvent<{ accent?: string }>).detail;
      if (detail?.accent) setAccent(detail.accent);
      else setAccent(currentThemeAccent());
    }
    window.addEventListener(THEME_CHANGED, onTheme);
    setAccent(currentThemeAccent());
    return () => window.removeEventListener(THEME_CHANGED, onTheme);
  }, []);

  useEffect(() => {
    function onLogStats(ev: Event) {
      const detail = (ev as CustomEvent<McpTraceStats>).detail;
      if (detail) setLogStats(detail);
    }
    window.addEventListener(MCP_TRACE_STATS, onLogStats);
    return () => window.removeEventListener(MCP_TRACE_STATS, onLogStats);
  }, []);

  useEffect(() => {
    if (!isLogging) setLogStats(emptyMcpTraceStats());
  }, [isLogging]);

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
    navigateTo(page, params);
  }

  const center = pageContext.detail
    ? `${pageContext.view} · ${pageContext.detail}`
    : pageContext.view;

  const unresolvedCount = stats?.unresolved_ref_count ?? 0;
  const qTone = gradeTone(quality.score);
  const qualityActive = quality.verbosePresent || quality.verboseEnabled || quality.score > 0;

  // Inline accent so the bar cannot fall back to a grey UA / cascade color.
  const barStyle = offline
    ? {
        backgroundColor: `color-mix(in srgb, ${accent} 35%, #6b3030)`,
        borderTopColor: `color-mix(in srgb, ${accent} 40%, #a05050)`,
      }
    : {
        backgroundColor: accent,
        borderTopColor: accent,
      };

  return (
    <footer
      className={`statusbar status-bar${isLogging ? ' statusbar--logging' : ''}${
        offline ? ' statusbar--offline' : ''
      }`}
      aria-label="Status"
      ref={barRef}
      style={barStyle}
      data-accent={accent}
    >
      {isLogging ? (
        <div className="statusbar-left">
          <StatusChip
            id="log-project"
            title="Switch project MCP log"
            openPanel={openPanel}
            onTogglePanel={setOpenPanel}
            className="status-project"
            panel={
              <div className="status-logging-switch-panel">
                <div className="status-panel-title">Project log</div>
                <p className="status-panel-text">
                  Switch the active workspace to follow another project&apos;s{' '}
                  <code>.ax/mcp-verbose.log</code>.
                </p>
                <LoggingProjectSwitch
                  currentPath={logStats.projectRoot}
                  currentLabel={logStats.projectLabel}
                  variant="inline"
                  onSwitched={() => setOpenPanel(null)}
                />
                <PanelLink
                  label="Browse all projects →"
                  onClick={() => {
                    setOpenPanel(null);
                    setProjectModalOpen(true);
                  }}
                />
              </div>
            }
          >
            <Codicon name="symbol-structure" className="status-project-icon" />
            <span className="status-lbl">{logStats.projectLabel || '—'}</span>
          </StatusChip>
          <span className="status-sep" aria-hidden="true">
            |
          </span>
          <StatusChip
            id="log-in"
            title="Filter inbound events (click again to toggle)"
            openPanel={openPanel}
            onTogglePanel={setOpenPanel}
            onNavigate={() => publishMcpTraceFilter({ toggleKind: 'inbound' })}
          >
            <span className="status-val status-log-kind status-log-kind--inbound">
              {logStats.inbound.toLocaleString()}
            </span>
            <span className="status-lbl">in</span>
          </StatusChip>
          <span className="status-sep" aria-hidden="true">
            |
          </span>
          <StatusChip
            id="log-out"
            title="Filter outbound events (click again to toggle)"
            openPanel={openPanel}
            onTogglePanel={setOpenPanel}
            onNavigate={() => publishMcpTraceFilter({ toggleKind: 'outbound' })}
          >
            <span className="status-val status-log-kind status-log-kind--outbound">
              {logStats.outbound.toLocaleString()}
            </span>
            <span className="status-lbl">out</span>
          </StatusChip>
          <span className="status-sep" aria-hidden="true">
            |
          </span>
          <StatusChip
            id="log-prev"
            title="Filter preview events (click again to toggle)"
            openPanel={openPanel}
            onTogglePanel={setOpenPanel}
            onNavigate={() => publishMcpTraceFilter({ toggleKind: 'preview' })}
          >
            <span className="status-val status-log-kind status-log-kind--preview">
              {logStats.preview.toLocaleString()}
            </span>
            <span className="status-lbl">prev</span>
          </StatusChip>
          <span className="status-sep" aria-hidden="true">
            |
          </span>
          <StatusChip
            id="log-err"
            title="Filter error events (click again to toggle)"
            openPanel={openPanel}
            onTogglePanel={setOpenPanel}
            onNavigate={() => publishMcpTraceFilter({ toggleKind: 'error' })}
            className={logStats.error > 0 ? 'status-warn' : ''}
          >
            <span className="status-val status-log-kind status-log-kind--error">
              {logStats.error.toLocaleString()}
            </span>
            <span className="status-lbl">err</span>
          </StatusChip>
          <span className="status-sep" aria-hidden="true">
            |
          </span>
          <StatusChip
            id="log-total"
            title="Total events in live buffer (capped)"
            openPanel={openPanel}
            onTogglePanel={setOpenPanel}
            panel={
              <>
                <div className="status-panel-title">Buffer breakdown</div>
                <p className="status-panel-text">Click a kind to filter the Logging table.</p>
                <ul className="status-panel-list">
                  {(
                    [
                      ['inbound', 'Inbound', logStats.inbound],
                      ['outbound', 'Outbound', logStats.outbound],
                      ['preview', 'Preview', logStats.preview],
                      ['error', 'Error', logStats.error],
                      ['internal', 'Internal', logStats.internal],
                      ['enrich', 'Enrich', logStats.enrich],
                      ['other', 'Other', logStats.other],
                    ] as const
                  ).map(([kind, label, count]) => (
                    <li key={kind}>
                      <button
                        type="button"
                        className="status-panel-list-btn"
                        onClick={() => {
                          publishMcpTraceFilter({ toggleKind: kind as TraceKind });
                          setOpenPanel(null);
                        }}
                      >
                        <span>{label}</span>
                        <span className="status-panel-list-val">{count.toLocaleString()}</span>
                      </button>
                    </li>
                  ))}
                </ul>
                <button
                  type="button"
                  className="status-panel-link"
                  onClick={() => {
                    publishMcpTraceFilter({ clear: true });
                    setOpenPanel(null);
                  }}
                >
                  Clear log filters
                </button>
                {logStats.path && (
                  <p className="status-panel-muted mono" title={logStats.path}>
                    {logStats.path}
                  </p>
                )}
              </>
            }
          >
            <IconLog />
            <span className="status-val">{logStats.total.toLocaleString()}</span>
            <span className="status-lbl">events</span>
          </StatusChip>
        </div>
      ) : (
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
      )}

      {isLogging ? (
        <div className="statusbar-center" title={center}>
          {center}
        </div>
      ) : (
        <button
          type="button"
          className="statusbar-center statusbar-center--clickable"
          title={`${center} — open Stats`}
          onClick={() => nav('stats')}
        >
          {center}
        </button>
      )}

      <div className="statusbar-right">
        {(isLogging || qualityActive) && (
          <>
            <button
              type="button"
              className={`status-item status-item--clickable status-quality status-quality--${qTone}${
                quality.criticalCount > 0 ? ' status-quality--crit' : ''
              }`}
              title="MCP quality loop — open metrics slide-out"
              aria-label={`MCP quality score ${quality.score}`}
              onClick={() => openMcpQualitySlideout()}
            >
              <IconShield />
              <span className="status-lbl">Q</span>
              <span className="status-val">{quality.score || '—'}</span>
              {quality.criticalCount > 0 && (
                <span className="status-quality-badge">{quality.criticalCount}</span>
              )}
            </button>
            <span className="status-sep" aria-hidden="true">
              |
            </span>
          </>
        )}
        {isLogging ? (
          <>
            <span
              className={`status-item status-logging${logStats.live ? ' status-logging--hot' : ''}`}
              title={logStats.live ? 'SSE connected' : 'Stream offline / reconnecting'}
            >
              <IconLog />
              <span className="status-lbl">Logging</span>
              <span
                className={`status-val status-logging-info${
                  logStats.live ? ' status-logging-info--live' : ''
                }`}
              >
                {logStats.live ? 'live' : 'offline'}
              </span>
            </span>
            <span className="status-sep" aria-hidden="true">
              |
            </span>
            <button
              type="button"
              className="status-item status-item--clickable status-project"
              title={`${logStats.projectLabel || 'Project'} — browse for ax projects`}
              aria-label={`Switch project (${logStats.projectLabel || 'project'})`}
              onClick={() => setProjectModalOpen(true)}
            >
              <Codicon name="folder-opened" className="status-project-icon" />
              <span className="status-lbl">Switch</span>
            </button>
            {projectModalOpen && (
              <ProjectBrowserModal
                onClose={() => setProjectModalOpen(false)}
                onSwitched={() => setProjectModalOpen(false)}
              />
            )}
            {version && (
              <span className="status-item" title="ax version">
                <span className="status-lbl">v{version}</span>
              </span>
            )}
          </>
        ) : (
          <>
            <StatusChip
              id="logging"
              title="MCP verbose logging — open live Logging page"
              openPanel={openPanel}
              onTogglePanel={setOpenPanel}
              onNavigate={() => {
                nav('logging');
                // User gesture: request browser fullscreen for the Logging page.
                try {
                  if (!document.fullscreenElement) {
                    void document.documentElement.requestFullscreen();
                  }
                } catch {
                  // Overlay maximize still applies if the browser blocks FS.
                }
              }}
              className={
                mcpTrace.recent
                  ? 'status-logging status-logging--hot'
                  : 'status-logging'
              }
            >
              <IconLog />
              <span className="status-lbl">Logging</span>
              <span
                className={`status-val status-logging-info${mcpTrace.recent ? ' status-logging-info--live' : ''}`}
              >
                {mcpTrace.info}
              </span>
              {mcpTrace.connected && mcpTrace.recent && (
                <span className="settings-log-live status-logging-dot" aria-hidden="true">
                  ·
                </span>
              )}
            </StatusChip>
            <span className="status-sep" aria-hidden="true">
              |
            </span>
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
                aria-label={`Switch project (${stats.project_name})`}
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
          </>
        )}
      </div>
    </footer>
  );
}
