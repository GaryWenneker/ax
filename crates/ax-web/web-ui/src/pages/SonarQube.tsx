import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';

import {
  BusyLabel,
  PageCard,
  PageCardBody,
  PageHero,
  PageShell,
  PageStack,
  PageToasts,
} from '../components/ui/PageLayout';
import { Spinner } from '../components/ui/Spinner';
import { usePageContext } from '../context/UiContext';
import { DEFAULT_SONAR_CONFIG, SONAR_GUIDE_SECTIONS } from '../lib/sonarGuide';
import {
  bootstrapSonar,
  discoverSonar,
  fetchShipConfig,
  fetchSonarUiInfo,
  regenerateSonarToken,
  saveShipConfig,
  SONAR_UI_PROXY,
  streamSonarInstall,
  streamSonarStart,
  streamSonarStop,
  validateSonarToken,
  type ShipConfig,
  type SonarDiscovery,
  type SonarSetupStatus,
  type SonarStreamEvent,
} from '../shipApi';

const DEFAULT_CONFIG: ShipConfig = {
  ship: { target_branch: 'main', web_port: 7070, git_root: '', git_roots: [] },
  quality_gate: {
    steps: ['index', 'tia', 'tests', 'sonar', 'policy'],
    tests: { runner: 'cargo test' },
    index_mode: 'incremental',
  },
  remote: { provider: 'azure_devops' },
  sonar: { ...DEFAULT_SONAR_CONFIG },
  ui: { show_savings: true, show_agent_terminal: true },
  reviewers: {},
};


function StatusSkeleton() {
  return (
    <div className="settings-status-grid settings-status-grid--skeleton" aria-busy="true" aria-label="Loading">
      {Array.from({ length: 6 }).map((_, i) => (
        <div key={i} className="skeleton-line skeleton-line--status" />
      ))}
    </div>
  );
}

function SettingRow({
  title,
  description,
  locked,
  children,
}: {
  title: ReactNode;
  description?: string;
  locked?: string;
  children: ReactNode;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-label">
        <span className="settings-row-title">{title}</span>
        {description && <span className="settings-row-desc">{description}</span>}
        {locked && <span className="settings-row-locked">{locked}</span>}
      </div>
      <div className="settings-row-control">{children}</div>
    </div>
  );
}

function Toggle({
  checked,
  onChange,
  label,
  disabled = false,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      aria-disabled={disabled}
      disabled={disabled}
      className={`settings-toggle${checked ? ' on' : ''}${disabled ? ' disabled' : ''}`}
      onClick={() => !disabled && onChange(!checked)}
    >
      <span className="settings-toggle-thumb" />
    </button>
  );
}

function StatusPill({
  label,
  value,
  tone = 'neutral',
  live,
}: {
  label: string;
  value: string;
  tone?: 'ok' | 'warn' | 'neutral';
  live?: boolean;
}) {
  const isLive = live ?? /running|scanning|starting|installing|stopping|loading|setting up|regenerating/i.test(value);
  return (
    <div className={`settings-status-pill${isLive ? ' settings-status-pill--live' : ''}`}>
      <span
        className={`settings-status-dot settings-status-dot--${tone}${isLive ? ' settings-status-dot--live' : ''}`}
        aria-hidden="true"
      />
      <div className="settings-status-pill-body">
        <span className="settings-status-pill-label">{label}</span>
        <span className="settings-status-pill-value">{value}</span>
      </div>
    </div>
  );
}

function LogConsole({
  lines,
  active,
  title = 'Operation log',
}: {
  lines: string[];
  active: boolean;
  title?: string;
}) {
  const tailRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    tailRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [lines]);

  if (lines.length === 0 && !active) return null;

  return (
    <div className="settings-log-panel">
      <div className="settings-log-header">
        <span>{title}</span>
        {active && <span className="settings-log-live">live</span>}
      </div>
      <pre className="settings-log-body" aria-live="polite">
        {lines.length === 0 ? 'Waiting for output…' : lines.join('\n')}
        <div ref={tailRef} />
      </pre>
    </div>
  );
}

function GuideSection({ title, body }: { title: string; body: string }) {
  return (
    <section className="sonar-guide-section">
      <h3 className="sonar-guide-section-title">{title}</h3>
      <div className="sonar-guide-section-body">
        {body.split('\n').map((line, i) => (
          <p key={i}>{line}</p>
        ))}
      </div>
    </section>
  );
}

export default function SonarQubePage() {
  const [tab, setTab] = useState<'dashboard' | 'setup'>('dashboard');
  const [iframeKey, setIframeKey] = useState(0);
  const [iframeState, setIframeState] = useState<'loading' | 'loaded' | 'error'>('loading');
  const [config, setConfig] = useState<ShipConfig>(DEFAULT_CONFIG);
  const [configLoaded, setConfigLoaded] = useState(false);
  const [discovery, setDiscovery] = useState<SonarDiscovery | null>(null);
  const [sonarSetup, setSonarSetup] = useState<SonarSetupStatus | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [sonarLogs, setSonarLogs] = useState<string[]>([]);
  const abortRef = useRef<AbortController | null>(null);
  const iframeLoadTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [tokenCheck, setTokenCheck] = useState<
    'idle' | 'checking' | 'valid' | 'invalid' | 'missing' | 'unreachable' | 'error'
  >('idle');
  const [tokenCheckMsg, setTokenCheckMsg] = useState<string | null>(null);
  const configRef = useRef(config);
  configRef.current = config;

  const tokenInputClass =
    tokenCheck === 'valid'
      ? 'settings-input--valid'
      : tokenCheck === 'invalid' || tokenCheck === 'missing' || tokenCheck === 'error'
        ? 'settings-input--invalid'
        : '';

  useEffect(() => {
    if (tab === 'dashboard') {
      setIframeState('loading');
      setIframeKey((k) => k + 1);
    }
  }, [tab]);

  useEffect(() => {
    if (tab !== 'dashboard' || iframeState !== 'loading') {
      if (iframeLoadTimer.current) {
        clearTimeout(iframeLoadTimer.current);
        iframeLoadTimer.current = null;
      }
      return;
    }
    // Dismiss loading overlay quickly — iframe stays interactive underneath (pointer-events: none on overlay).
    iframeLoadTimer.current = setTimeout(() => {
      setIframeState((s) => (s === 'loading' ? 'loaded' : s));
    }, 600);
    return () => {
      if (iframeLoadTimer.current) {
        clearTimeout(iframeLoadTimer.current);
        iframeLoadTimer.current = null;
      }
    };
  }, [tab, iframeState, iframeKey]);

  usePageContext('SonarQube', tab === 'dashboard' ? 'Dashboard · dark mode proxy' : 'Setup · quality gate');

  const runTokenCheck = useCallback(async (reachableOverride?: boolean) => {
    const reachable = reachableOverride ?? discovery?.reachable;
    if (!reachable) {
      setTokenCheck('idle');
      setTokenCheckMsg(null);
      return;
    }

    setTokenCheck('checking');
    setTokenCheckMsg('Checking scanner token…');

    try {
      const r = await validateSonarToken();
      if (!r.ok) {
        setTokenCheck('error');
        setTokenCheckMsg(r.error ?? 'Check failed');
        return;
      }
      if (!r.reachable) {
        setTokenCheck('unreachable');
        setTokenCheckMsg(r.message ?? 'SonarQube is not reachable');
        return;
      }
      if (!r.configured) {
        setTokenCheck('missing');
        setTokenCheckMsg(r.message ?? 'No scanner token');
        return;
      }
      if (r.valid) {
        setTokenCheck('valid');
        setTokenCheckMsg(r.message ?? 'Scanner token is valid');
      } else {
        setTokenCheck('invalid');
        setTokenCheckMsg(
          r.message ?? 'Scanner token rejected — run Setup project & token to regenerate',
        );
      }
    } catch (e: unknown) {
      setTokenCheck('error');
      setTokenCheckMsg(String(e));
    }
  }, [discovery?.reachable]);

  const refreshDiscovery = useCallback(async () => {
    try {
      const d = await discoverSonar();
      setDiscovery(d.discovery);
      if (d.setup) setSonarSetup(d.setup);
      await runTokenCheck(d.discovery.reachable);
      await fetchSonarUiInfo().catch(() => null);
    } catch (e) {
      setErr(String(e));
    }
  }, [runTokenCheck]);

  const loadShipAndSonar = useCallback(async () => {
    setErr(null);
    try {
      const d = await fetchShipConfig();
      setConfig({
        ...DEFAULT_CONFIG,
        ...d.config,
        ship: { ...DEFAULT_CONFIG.ship, ...d.config.ship },
        ui: { ...DEFAULT_CONFIG.ui, ...d.config.ui },
        quality_gate: { ...DEFAULT_CONFIG.quality_gate, ...d.config.quality_gate },
        sonar: { ...DEFAULT_CONFIG.sonar, ...d.config.sonar },
      });
      setDiscovery(d.sonar);
      if (d.sonar_setup) setSonarSetup(d.sonar_setup);
      setConfigLoaded(true);

      // Live discovery in background — do not block the iframe.
      void discoverSonar()
        .then((live) => {
          setDiscovery(live.discovery);
          if (live.setup) setSonarSetup(live.setup);
        })
        .catch(() => null);
    } catch (e) {
      setErr(String(e));
      setDiscovery(null);
      setConfigLoaded(true);
    }
  }, []);

  // Token validation hits SonarQube APIs (~4s) — defer until Setup tab so Dashboard stays snappy.
  useEffect(() => {
    if (tab !== 'setup' || !configLoaded || !discovery?.reachable) return;
    const t = setTimeout(() => void runTokenCheck(), 0);
    return () => clearTimeout(t);
  }, [tab, configLoaded, discovery?.reachable, runTokenCheck]);

  useEffect(() => {
    void loadShipAndSonar();
  }, [loadShipAndSonar]);

  async function save(nextConfig: ShipConfig = config) {
    setBusy('save');
    setErr(null);
    setMsg(null);
    try {
      await saveShipConfig(nextConfig);
      setConfig(nextConfig);
      setMsg('Settings saved to .ax/ship.toml');
      await refreshDiscovery();
    } catch (e) {
      setErr(String(e));
      throw e;
    } finally {
      setBusy(null);
    }
  }

  async function setSonarEnabled(enabled: boolean) {
    const next = { ...config, sonar: { ...config.sonar, enabled } };
    setConfig(next);
    setErr(null);
    setMsg(null);
    try {
      await saveShipConfig(next);
      setMsg(enabled ? 'SonarQube enabled for the quality gate' : 'SonarQube disabled');
    } catch (e) {
      setConfig((c) => ({ ...c, sonar: { ...c.sonar, enabled: !enabled } }));
      setErr(String(e));
    }
  }

  useEffect(() => {
    return () => abortRef.current?.abort();
  }, []);

  async function runSonarStream(
    action: 'install' | 'start' | 'stop',
    streamFn: (
      onEvent: (ev: SonarStreamEvent) => void,
      signal?: AbortSignal,
    ) => Promise<void>,
  ) {
    abortRef.current?.abort();
    const ac = new AbortController();
    abortRef.current = ac;

    setBusy(action);
    setErr(null);
    setMsg(null);
    setSonarLogs([]);

    const append = (line: string) => setSonarLogs((prev) => [...prev, line]);

    try {
      await streamFn((ev) => {
        if (ev.type === 'log') {
          append(ev.line);
        }
        if (ev.type === 'done') {
          if (ev.logs?.length) setSonarLogs(ev.logs);
          if (!ev.ok) throw new Error(ev.error ?? `${action} failed`);
          if (action === 'install') {
            setConfig((c) => ({ ...c, sonar: { ...c.sonar, enabled: true } }));
          }
          if (ev.discovery) setDiscovery(ev.discovery);
          setMsg(
            action === 'install'
              ? 'SonarQube is live — projects and tokens provision automatically'
              : action === 'start'
                ? 'SonarQube started'
                : 'SonarQube stopped',
          );
          void refreshDiscovery();
        }
      }, ac.signal);
    } catch (e) {
      if (ac.signal.aborted) return;
      setErr(String(e));
    } finally {
      if (!ac.signal.aborted) setBusy(null);
    }
  }

  async function install() {
    await runSonarStream('install', streamSonarInstall);
  }

  async function start() {
    await runSonarStream('start', streamSonarStart);
  }

  async function stop() {
    await runSonarStream('stop', streamSonarStop);
  }

  async function regenerateToken() {
    setBusy('regenerate-token');
    setErr(null);
    setMsg(null);
    try {
      await saveShipConfig(config);
      const r = await regenerateSonarToken();
      if (!r.ok) throw new Error(r.error ?? 'Token regeneration failed');
      if (r.setup) setSonarSetup(r.setup);
      setMsg(r.result?.message ?? 'Scanner token regenerated');
      await runTokenCheck(true);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function setupSonar() {
    setBusy('bootstrap');
    setErr(null);
    setMsg(null);
    try {
      await saveShipConfig(config);
      const r = await bootstrapSonar();
      if (!r.ok) throw new Error(r.error ?? 'SonarQube setup failed');
      if (r.setup) setSonarSetup(r.setup);
      if (r.config) setConfig(r.config);
      else if (r.result?.project_key) {
        setConfig((c) => ({
          ...c,
          sonar: { ...c.sonar, project_key: r.result!.project_key },
        }));
      }
      setMsg(r.result?.message ?? 'SonarQube project and token configured');
      await refreshDiscovery();
      await runTokenCheck();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  }

  const containerRunning = discovery?.container?.running === true;
  const containerExists = !!discovery?.container;
  const sonarInfraLocked = containerExists;
  const sonarInfraLockReason = containerRunning
    ? 'Locked while the container is running'
    : 'Stop the container before changing host, name, or runtime';
  const logActive = busy === 'install' || busy === 'start' || busy === 'stop';
  const logTitle =
    busy === 'install' ? 'Install log' : busy === 'stop' ? 'Stop log' : busy === 'start' ? 'Start log' : 'Operation log';

  const runtimeSummary = discovery?.runtimes.length
    ? discovery.runtimes.map((r) => `${r.runtime} ${r.version}`).join(' · ')
    : 'None detected';

  const containerSummary = discovery?.container
    ? `${discovery.container.name} (${discovery.container.status})`
    : 'Not found';

  const apiTone = discovery?.reachable ? 'ok' : 'warn';
  const apiSummary = discovery?.reachable ? 'Reachable' : 'Not reachable';

  const canInstall = !containerExists && !busy;
  const canStart = containerExists && !containerRunning && !busy;
  const canStop = containerRunning && !busy;
  const repoProjects = sonarSetup?.repo_projects ?? [];
  const readyRepos = repoProjects.filter((p) => p.exists).length;
  const needsBootstrap =
    discovery?.reachable &&
    (!sonarSetup ||
      !sonarSetup.project_exists ||
      (repoProjects.length > 0 && readyRepos < repoProjects.length) ||
      tokenCheck === 'invalid' ||
      tokenCheck === 'missing' ||
      !sonarSetup.token_configured ||
      sonarSetup.token_valid === false);

  const canRegenerateToken =
    discovery?.reachable && (tokenCheck === 'invalid' || tokenCheck === 'missing');

  const projectPillValue = repoProjects.length
    ? `${readyRepos}/${repoProjects.length} projects`
    : sonarSetup?.project_exists
      ? config.sonar.project_key
      : sonarSetup?.project_lookup === 'auth_failed'
        ? 'auth failed'
        : sonarSetup?.project_lookup === 'unreachable'
          ? 'offline'
          : 'missing';

  const tokenPillValue =
    tokenCheck === 'valid'
      ? 'valid'
      : tokenCheck === 'invalid'
        ? 'invalid'
        : tokenCheck === 'missing'
          ? 'missing'
          : tokenCheck === 'checking'
            ? 'checking…'
            : !sonarSetup?.token_configured
              ? 'missing'
              : sonarSetup.token_valid === false
                ? 'invalid'
                : 'configured';

  const tokenPillTone =
    tokenCheck === 'valid' ||
    (tokenCheck === 'idle' && sonarSetup?.token_configured && sonarSetup.token_valid !== false)
      ? 'ok'
      : 'warn';

  const dashboardEntry = `${SONAR_UI_PROXY}projects`;

  return (
    <PageShell className="sonar-page">
      <PageHero
        title="SonarQube"
        subtitle={
          <>
            SonarQube runs inside Command Center via a reverse proxy — automatic login and dark theme.
            Setup and container management live on the Setup tab.
          </>
        }
        actions={
          <div className="sonar-page-tabs" role="tablist" aria-label="SonarQube views">
            <button
              type="button"
              role="tab"
              aria-selected={tab === 'dashboard'}
              className={`btn${tab === 'dashboard' ? ' primary' : ' btn-subtle'}`}
              onClick={() => setTab('dashboard')}
            >
              Dashboard
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={tab === 'setup'}
              className={`btn${tab === 'setup' ? ' primary' : ' btn-subtle'}`}
              onClick={() => setTab('setup')}
            >
              Setup
            </button>
            {tab === 'dashboard' && (
              <>
                <a className="btn btn-subtle" href={dashboardEntry} target="_blank" rel="noreferrer">
                  Open in new tab
                </a>
                <button
                  type="button"
                  className="btn btn-subtle"
                  onClick={() => {
                    setIframeState('loading');
                    setIframeKey((k) => k + 1);
                  }}
                >
                  Reload
                </button>
              </>
            )}
          </div>
        }
      />

      <PageToasts ok={msg} err={err} />

      {tab === 'dashboard' && (
        <section className="sonar-dashboard-panel">
          <div className="sonar-dashboard-frame-wrap">
            {iframeState === 'loading' && (
              <div className="sonar-dashboard-loading" aria-live="polite">
                <Spinner size="md" />
                <span>Loading SonarQube…</span>
              </div>
            )}
            {iframeState === 'error' && (
              <div className="sonar-dashboard-offline">
                <p>Dashboard failed to load. Try Reload or open in a new tab.</p>
                <a className="btn primary" href={dashboardEntry} target="_blank" rel="noreferrer">
                  Open in new tab
                </a>
              </div>
            )}
            <iframe
              key={iframeKey}
              className={`sonar-dashboard-frame${iframeState !== 'error' ? ' sonar-dashboard-frame--visible' : ''}`}
              title="SonarQube dashboard"
              src={dashboardEntry}
              referrerPolicy="no-referrer"
              onLoad={() => setIframeState('loaded')}
              onError={() => setIframeState('error')}
            />
          </div>
        </section>
      )}

      {tab === 'setup' && (
      <PageStack>
        <PageCard
          title="Guide"
          description="Setup notes, dark mode, and how ax provisions SonarQube without manual login."
        >
          <PageCardBody>
            <div className="sonar-guide">
              {SONAR_GUIDE_SECTIONS.map((section) => (
                <GuideSection key={section.id} title={section.title} body={section.body} />
              ))}
            </div>
          </PageCardBody>
        </PageCard>

        <PageCard
          title="Stack & quality gate"
          description="PostgreSQL-backed container stack. Install upgrades legacy embedded-H2 containers automatically."
        >
          <PageCardBody>
            <SettingRow
              title="Enable SonarQube"
              description="Include SonarQube in the Command Center quality gate."
            >
              <Toggle
                label="Enable SonarQube"
                checked={config.sonar.enabled}
                disabled={!!busy}
                onChange={setSonarEnabled}
              />
            </SettingRow>

            <div className="settings-divider" />

            <SettingRow title="Host" description="SonarQube server URL." locked={sonarInfraLocked ? sonarInfraLockReason : undefined}>
              <input
                className="settings-input"
                value={config.sonar.host}
                disabled={sonarInfraLocked || !!busy}
                onChange={(e) => setConfig({ ...config, sonar: { ...config.sonar, host: e.target.value } })}
              />
            </SettingRow>

            <SettingRow
              title="Container name"
              description="Podman or Docker container to manage."
              locked={sonarInfraLocked ? sonarInfraLockReason : undefined}
            >
              <input
                className="settings-input"
                value={config.sonar.podman_container ?? 'sonarqube'}
                disabled={sonarInfraLocked || !!busy}
                onChange={(e) =>
                  setConfig({ ...config, sonar: { ...config.sonar, podman_container: e.target.value } })
                }
              />
            </SettingRow>

            <SettingRow
              title="Runtime"
              description="Container engine preference."
              locked={sonarInfraLocked ? sonarInfraLockReason : undefined}
            >
              <select
                className="settings-select"
                value={config.sonar.container_runtime}
                disabled={sonarInfraLocked || !!busy}
                onChange={(e) =>
                  setConfig({ ...config, sonar: { ...config.sonar, container_runtime: e.target.value } })
                }
              >
                <option value="auto">Auto (Podman → Docker)</option>
                <option value="podman">Podman</option>
                <option value="docker">Docker</option>
              </select>
            </SettingRow>

            <SettingRow title="Project key" description="SonarQube project identifier.">
              <input
                className="settings-input"
                value={config.sonar.project_key}
                onChange={(e) =>
                  setConfig({ ...config, sonar: { ...config.sonar, project_key: e.target.value } })
                }
              />
            </SettingRow>

            <SettingRow title="Token env var" description="Environment variable for the scanner token. Set automatically by setup.">
              <input
                className="settings-input settings-input--mono"
                value={config.sonar.token_env}
                onChange={(e) =>
                  setConfig({ ...config, sonar: { ...config.sonar, token_env: e.target.value } })
                }
              />
            </SettingRow>

            <SettingRow title="Scan scope" description="Incremental scans changed files only; full scans the entire project.">
              <select
                className="settings-select"
                value={config.sonar.scan_mode ?? 'incremental'}
                onChange={(e) =>
                  setConfig({ ...config, sonar: { ...config.sonar, scan_mode: e.target.value } })
                }
              >
                <option value="incremental">Incremental (changed files)</option>
                <option value="full">Full codebase</option>
              </select>
            </SettingRow>

            <div className="settings-divider" />

            <SettingRow
              title="Admin access"
              description={
                sonarSetup?.login_password_hint ??
                'Default local admin credentials in .ax/ship.toml — applied automatically by ax.'
              }
            >
              <span className="settings-input settings-input--mono" style={{ border: 'none', background: 'transparent', padding: 0 }}>
                {sonarSetup?.login_user ? `User: ${sonarSetup.login_user} · managed by ax` : 'Managed by ax'}
              </span>
            </SettingRow>

            <SettingRow
              title="Scanner token"
              description={`Stored in ${sonarSetup?.token_path ?? '.ax/sonar.token'} — validated live against SonarQube.`}
            >
              <div className="settings-field-check">
                <span
                  className={`settings-input settings-input--mono ${tokenInputClass}`}
                  style={{ display: 'inline-flex', alignItems: 'center', border: 'none', background: 'transparent', padding: 0 }}
                >
                  {tokenCheck === 'checking'
                    ? 'Checking…'
                    : tokenCheck === 'valid'
                      ? 'Valid'
                      : tokenCheck === 'invalid'
                        ? 'Invalid'
                        : tokenCheck === 'missing'
                          ? 'Missing'
                          : tokenCheck === 'unreachable'
                            ? 'Unreachable'
                            : sonarSetup?.token_configured
                              ? 'Configured'
                              : 'Not set'}
                </span>
                {tokenCheck !== 'idle' && tokenCheckMsg && (
                  <span
                    className={`settings-field-check-msg settings-field-check-msg--${tokenCheck}`}
                    role="status"
                    aria-live="polite"
                  >
                    {tokenCheckMsg}
                  </span>
                )}
              </div>
            </SettingRow>
          </PageCardBody>

          <div className="settings-status-panel">
            <div className="settings-status-panel-title">Runtime status</div>
            {!configLoaded ? (
              <StatusSkeleton />
            ) : (
              <div className="settings-status-grid">
                <StatusPill label="Runtimes" value={runtimeSummary} tone="neutral" />
                <StatusPill
                  label="Container"
                  value={containerSummary}
                  tone={containerRunning ? 'ok' : discovery?.container ? 'warn' : 'warn'}
                />
                <StatusPill label="API" value={apiSummary} tone={apiTone} />
                {sonarSetup && (
                  <>
                    <StatusPill
                      label="Project"
                      value={projectPillValue}
                      tone={sonarSetup.project_exists ? 'ok' : 'warn'}
                    />
                    {repoProjects.length > 0 && (
                      <StatusPill
                        label="Repos"
                        value={repoProjects.map((p) => p.name).join(', ')}
                        tone="neutral"
                      />
                    )}
                    <StatusPill label="Token" value={tokenPillValue} tone={tokenPillTone} />
                    <StatusPill
                      label="Scanner"
                      value={sonarSetup.scanner_available ? 'local' : 'via container'}
                      tone="neutral"
                    />
                  </>
                )}
              </div>
            )}
          </div>

          <LogConsole lines={sonarLogs} active={logActive} title={logTitle} />

          <div className="settings-card-footer">
            {canInstall && (
              <button type="button" className="btn primary" disabled={!!busy} onClick={install}>
                {busy === 'install' ? <BusyLabel label="Installing…" /> : 'Install & start SonarQube'}
              </button>
            )}
            {canStart && (
              <button type="button" className="btn primary" disabled={!!busy} onClick={start}>
                {busy === 'start' ? <BusyLabel label="Starting…" /> : 'Start container'}
              </button>
            )}
            {canStop && (
              <button type="button" className="btn btn-subtle" disabled={!!busy} onClick={stop}>
                {busy === 'stop' ? <BusyLabel label="Stopping…" /> : 'Stop container'}
              </button>
            )}
            {canRegenerateToken && (
              <button type="button" className="btn primary" disabled={!!busy} onClick={regenerateToken}>
                {busy === 'regenerate-token' ? 'Regenerating…' : 'Regenerate token'}
              </button>
            )}
            {needsBootstrap && (
              <button type="button" className="btn primary" disabled={!!busy} onClick={setupSonar}>
                {busy === 'bootstrap' ? 'Setting up…' : 'Setup project & token'}
              </button>
            )}
            <button type="button" className="btn btn-subtle" disabled={!!busy} onClick={refreshDiscovery}>
              Refresh status
            </button>
            <button type="button" className="btn primary" disabled={!!busy} onClick={() => void save()}>
              {busy === 'save' ? <BusyLabel label="Saving…" /> : 'Save SonarQube settings'}
            </button>
          </div>
        </PageCard>
      </PageStack>
      )}
    </PageShell>
  );
}
