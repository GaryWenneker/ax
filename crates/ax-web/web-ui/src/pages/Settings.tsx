import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';

import { usePageContext } from '../context/UiContext';
import {
  bootstrapSonar,
  discoverSonar,
  fetchShipConfig,
  saveShipConfig,
  streamSonarInstall,
  streamSonarStart,
  streamSonarStop,
  type ShipConfig,
  type SonarDiscovery,
  type SonarSetupStatus,
  type SonarStreamEvent,
} from '../shipApi';

const DEFAULT_CONFIG: ShipConfig = {
  ship: { target_branch: 'main', web_port: 7070 },
  quality_gate: {
    steps: ['index', 'tia', 'tests', 'sonar', 'policy'],
    tests: { runner: 'cargo test' },
  },
  remote: { provider: 'azure_devops' },
  sonar: {
    enabled: false,
    host: 'http://localhost:9000',
    project_key: 'ax',
    token_env: 'SONAR_TOKEN',
    scanner_path: 'sonar-scanner',
    podman_container: 'sonarqube',
    container_runtime: 'auto',
  },
  reviewers: {},
};

function SettingRow({
  title,
  description,
  locked,
  children,
}: {
  title: string;
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
}: {
  label: string;
  value: string;
  tone?: 'ok' | 'warn' | 'neutral';
}) {
  return (
    <div className="settings-status-pill">
      <span className={`settings-status-dot settings-status-dot--${tone}`} aria-hidden="true" />
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

export default function SettingsPage() {
  const [config, setConfig] = useState<ShipConfig>(DEFAULT_CONFIG);
  const [discovery, setDiscovery] = useState<SonarDiscovery | null>(null);
  const [sonarSetup, setSonarSetup] = useState<SonarSetupStatus | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [sonarLogs, setSonarLogs] = useState<string[]>([]);
  const abortRef = useRef<AbortController | null>(null);

  usePageContext('Settings', 'Command Center · SonarQube');

  const refreshDiscovery = useCallback(async () => {
    try {
      const d = await discoverSonar();
      setDiscovery(d.discovery);
      if (d.setup) setSonarSetup(d.setup);
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    fetchShipConfig()
      .then((d) => {
        setConfig(d.config);
        setDiscovery(d.sonar);
        if (d.sonar_setup) setSonarSetup(d.sonar_setup);
      })
      .catch((e) => setErr(String(e)));
  }, []);

  async function save() {
    setBusy('save');
    setErr(null);
    setMsg(null);
    try {
      await saveShipConfig(config);
      setMsg('Settings saved to .ax/ship.toml');
      await refreshDiscovery();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
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
              ? 'SonarQube is live'
              : action === 'start'
                ? 'SonarQube started'
                : 'SonarQube stopped',
          );
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

  async function setupSonar() {
    setBusy('bootstrap');
    setErr(null);
    setMsg(null);
    try {
      const r = await bootstrapSonar();
      if (!r.ok) throw new Error(r.error ?? 'SonarQube setup failed');
      if (r.setup) setSonarSetup(r.setup);
      setMsg(r.result?.message ?? 'SonarQube project and token configured');
      await refreshDiscovery();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  }

  const az = config.remote.azure_devops ?? {
    org: '',
    project: '',
    repo_id: '',
    token_env: 'AZDO_PAT',
  };

  const containerRunning = discovery?.container?.running === true;
  const containerExists = !!discovery?.container;
  const sonarInfraLocked = containerExists;
  const sonarInfraLockReason = containerRunning
    ? 'Locked while the container is running'
    : 'Stop the container before changing host, name, or runtime';
  const commandCenterLive =
    typeof window !== 'undefined' &&
    (window.location.port === String(config.ship.web_port) ||
      (!window.location.port && config.ship.web_port === 80));
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
  const needsBootstrap =
    discovery?.reachable &&
    sonarSetup &&
    (!sonarSetup.project_exists || !sonarSetup.token_configured);

  return (
    <div className="page settings-page">
      <header className="settings-hero">
        <h1 className="settings-hero-title">Settings</h1>
        <p className="settings-hero-sub">
          Command Center & SonarQube configuration stored in <code>.ax/ship.toml</code>
        </p>
      </header>

      {msg && <div className="settings-toast settings-toast--ok">{msg}</div>}
      {err && <div className="settings-toast settings-toast--err">{err}</div>}

      <div className="settings-stack">
        <section className="settings-card">
          <div className="settings-card-header">
            <h2>SonarQube</h2>
            <p>Auto-detects Podman or Docker. One click pulls, creates, and starts the container.</p>
          </div>

          <div className="settings-card-body">
            <SettingRow
              title="Enable SonarQube"
              description="Include SonarQube in the Command Center quality gate."
            >
              <Toggle
                label="Enable SonarQube"
                checked={config.sonar.enabled}
                onChange={(enabled) => setConfig({ ...config, sonar: { ...config.sonar, enabled } })}
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

            <SettingRow title="Token env var" description="Environment variable for the scanner token. Auto-set by Setup.">
              <input
                className="settings-input settings-input--mono"
                value={config.sonar.token_env}
                onChange={(e) =>
                  setConfig({ ...config, sonar: { ...config.sonar, token_env: e.target.value } })
                }
              />
            </SettingRow>

            {sonarSetup && (
              <>
                <div className="settings-divider" />
                <div className="settings-subsection-label">Access & setup</div>
                <SettingRow title="UI login" description={sonarSetup.login_password_hint}>
                  <span className="settings-input settings-input--mono" style={{ display: 'inline-flex', alignItems: 'center', border: 'none', background: 'transparent', padding: 0 }}>
                    {sonarSetup.login_user} / admin
                  </span>
                </SettingRow>
                <SettingRow title="Open SonarQube" description="Local SonarQube dashboard.">
                  <a className="btn btn-subtle btn-compact" href={config.sonar.host} target="_blank" rel="noreferrer">
                    {config.sonar.host}
                  </a>
                </SettingRow>
              </>
            )}
          </div>

          <div className="settings-status-panel">
            <div className="settings-status-panel-title">Runtime status</div>
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
                    value={sonarSetup.project_exists ? config.sonar.project_key : 'missing'}
                    tone={sonarSetup.project_exists ? 'ok' : 'warn'}
                  />
                  <StatusPill
                    label="Token"
                    value={sonarSetup.token_configured ? 'configured' : 'missing'}
                    tone={sonarSetup.token_configured ? 'ok' : 'warn'}
                  />
                  <StatusPill
                    label="Scanner"
                    value={sonarSetup.scanner_available ? 'local' : 'via container'}
                    tone="neutral"
                  />
                </>
              )}
            </div>
          </div>

          <LogConsole lines={sonarLogs} active={logActive} title={logTitle} />

          <div className="settings-card-footer">
            {canInstall && (
              <button type="button" className="btn primary" disabled={!!busy} onClick={install}>
                {busy === 'install' ? 'Installing…' : 'Install & start SonarQube'}
              </button>
            )}
            {canStart && (
              <button type="button" className="btn primary" disabled={!!busy} onClick={start}>
                {busy === 'start' ? 'Starting…' : 'Start container'}
              </button>
            )}
            {canStop && (
              <button type="button" className="btn btn-subtle" disabled={!!busy} onClick={stop}>
                {busy === 'stop' ? 'Stopping…' : 'Stop container'}
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
          </div>
        </section>

        <section className="settings-card">
          <div className="settings-card-header">
            <h2>Command Center</h2>
            <p>Ship pipeline, quality gate, and pull request integration.</p>
          </div>

          <div className="settings-card-body">
            <div className="settings-subsection-label">Pipeline</div>

            <SettingRow title="Target branch" description="Base branch for ship comparisons.">
              <input
                className="settings-input"
                value={config.ship.target_branch}
                onChange={(e) =>
                  setConfig({ ...config, ship: { ...config.ship, target_branch: e.target.value } })
                }
              />
            </SettingRow>

            <SettingRow
              title="Dashboard port"
              description="Local port for the Command Center web UI."
              locked={commandCenterLive ? 'Locked while this web UI is running on this port' : undefined}
            >
              <input
                className="settings-input settings-input--narrow"
                type="number"
                value={config.ship.web_port}
                disabled={commandCenterLive || !!busy}
                onChange={(e) =>
                  setConfig({ ...config, ship: { ...config.ship, web_port: Number(e.target.value) } })
                }
              />
            </SettingRow>

            <SettingRow title="Test runner" description="Command executed during the tests quality step.">
              <input
                className="settings-input settings-input--wide"
                value={config.quality_gate.tests.runner}
                onChange={(e) =>
                  setConfig({
                    ...config,
                    quality_gate: { ...config.quality_gate, tests: { runner: e.target.value } },
                  })
                }
              />
            </SettingRow>

            <div className="settings-divider" />
            <div className="settings-subsection-label">Pull requests</div>

            <SettingRow title="PR provider" description="Where ship opens and updates pull requests.">
              <select
                className="settings-select"
                value={config.remote.provider}
                onChange={(e) => setConfig({ ...config, remote: { ...config.remote, provider: e.target.value } })}
              >
                <option value="azure_devops">Azure DevOps</option>
                <option value="github">GitHub</option>
              </select>
            </SettingRow>

            {config.remote.provider === 'azure_devops' && (
              <>
                <SettingRow title="Organization" description="Azure DevOps organization slug.">
                  <input
                    className="settings-input"
                    value={az.org}
                    onChange={(e) =>
                      setConfig({
                        ...config,
                        remote: { ...config.remote, azure_devops: { ...az, org: e.target.value } },
                      })
                    }
                  />
                </SettingRow>

                <SettingRow title="Project" description="Azure DevOps project name.">
                  <input
                    className="settings-input"
                    value={az.project}
                    onChange={(e) =>
                      setConfig({
                        ...config,
                        remote: { ...config.remote, azure_devops: { ...az, project: e.target.value } },
                      })
                    }
                  />
                </SettingRow>

                <SettingRow title="Repository ID" description="UUID of the Azure DevOps repository.">
                  <input
                    className="settings-input settings-input--wide settings-input--mono"
                    value={az.repo_id}
                    onChange={(e) =>
                      setConfig({
                        ...config,
                        remote: { ...config.remote, azure_devops: { ...az, repo_id: e.target.value } },
                      })
                    }
                  />
                </SettingRow>

                <SettingRow title="Token env var" description="Environment variable holding the Azure DevOps PAT.">
                  <input
                    className="settings-input settings-input--mono"
                    value={az.token_env}
                    onChange={(e) =>
                      setConfig({
                        ...config,
                        remote: { ...config.remote, azure_devops: { ...az, token_env: e.target.value } },
                      })
                    }
                  />
                </SettingRow>
              </>
            )}
          </div>

          <div className="settings-card-footer">
            <button type="button" className="btn primary" disabled={!!busy} onClick={save}>
              {busy === 'save' ? 'Saving…' : 'Save settings'}
            </button>
          </div>
        </section>
      </div>
    </div>
  );
}
