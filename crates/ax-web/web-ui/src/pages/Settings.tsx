import { useCallback, useEffect, useState } from 'react';

import { usePageContext } from '../context/UiContext';
import {
  discoverSonar,
  fetchShipConfig,
  installSonar,
  saveShipConfig,
  startSonar,
  type ShipConfig,
  type SonarDiscovery,
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

export default function SettingsPage() {
  const [config, setConfig] = useState<ShipConfig>(DEFAULT_CONFIG);
  const [discovery, setDiscovery] = useState<SonarDiscovery | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  usePageContext('Settings', 'Command Center · SonarQube');

  const refreshDiscovery = useCallback(async () => {
    try {
      const d = await discoverSonar();
      setDiscovery(d.discovery);
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    fetchShipConfig()
      .then((d) => {
        setConfig(d.config);
        setDiscovery(d.sonar);
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

  async function install() {
    setBusy('install');
    setErr(null);
    setMsg(null);
    try {
      const r = await installSonar();
      if (!r.ok) throw new Error(r.error ?? 'Install failed');
      setConfig((c) => ({ ...c, sonar: { ...c.sonar, enabled: true } }));
      if (r.discovery) setDiscovery(r.discovery);
      setMsg('SonarQube is live');
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function start() {
    setBusy('start');
    setErr(null);
    try {
      const r = await startSonar();
      if (!r.ok) throw new Error(r.error ?? 'Start failed');
      if (r.discovery) setDiscovery(r.discovery);
      setMsg('SonarQube started');
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

  return (
    <div className="page settings-page">
      <div className="page-header">
        <div>
          <h1 className="page-title">Settings</h1>
          <p className="muted">Command Center & SonarQube · <code>.ax/ship.toml</code></p>
        </div>
      </div>

      {msg && <div className="ship-banner ship-banner--ok">{msg}</div>}
      {err && <div className="ship-banner ship-banner--err">{err}</div>}

      <section className="settings-section">
        <h2>SonarQube</h2>
        <p className="muted">
          Auto-detects Podman or Docker. One click pulls, creates, and starts the container.
        </p>

        <div className="settings-grid">
          <label>
            <span>Enabled</span>
            <input
              type="checkbox"
              checked={config.sonar.enabled}
              onChange={(e) => setConfig({ ...config, sonar: { ...config.sonar, enabled: e.target.checked } })}
            />
          </label>
          <label>
            <span>Host</span>
            <input
              value={config.sonar.host}
              onChange={(e) => setConfig({ ...config, sonar: { ...config.sonar, host: e.target.value } })}
            />
          </label>
          <label>
            <span>Container name</span>
            <input
              value={config.sonar.podman_container ?? 'sonarqube'}
              onChange={(e) => setConfig({ ...config, sonar: { ...config.sonar, podman_container: e.target.value } })}
            />
          </label>
          <label>
            <span>Runtime</span>
            <select
              value={config.sonar.container_runtime}
              onChange={(e) => setConfig({ ...config, sonar: { ...config.sonar, container_runtime: e.target.value } })}
            >
              <option value="auto">Auto (Podman → Docker)</option>
              <option value="podman">Podman</option>
              <option value="docker">Docker</option>
            </select>
          </label>
          <label>
            <span>Project key</span>
            <input
              value={config.sonar.project_key}
              onChange={(e) => setConfig({ ...config, sonar: { ...config.sonar, project_key: e.target.value } })}
            />
          </label>
          <label>
            <span>Token env var</span>
            <input
              value={config.sonar.token_env}
              onChange={(e) => setConfig({ ...config, sonar: { ...config.sonar, token_env: e.target.value } })}
            />
          </label>
        </div>

        <dl className="sonar-status-grid">
          <div>
            <dt>Runtimes</dt>
            <dd>
              {discovery?.runtimes.length
                ? discovery.runtimes.map((r) => `${r.runtime} ${r.version}`).join(' · ')
                : 'none detected'}
            </dd>
          </div>
          <div>
            <dt>Container</dt>
            <dd>
              {discovery?.container
                ? `${discovery.container.name} (${discovery.container.status})`
                : 'not found'}
            </dd>
          </div>
          <div>
            <dt>API</dt>
            <dd className={discovery?.reachable ? 'ok' : 'warn'}>
              {discovery?.reachable ? 'reachable' : 'not reachable'}
            </dd>
          </div>
        </dl>

        <div className="page-actions">
          <button type="button" className="btn primary" disabled={!!busy} onClick={install}>
            {busy === 'install' ? 'Installing…' : 'Install & start SonarQube'}
          </button>
          <button type="button" className="btn" disabled={!!busy} onClick={start}>
            {busy === 'start' ? 'Starting…' : 'Start container'}
          </button>
          <button type="button" className="btn" disabled={!!busy} onClick={refreshDiscovery}>
            Refresh status
          </button>
        </div>
      </section>

      <section className="settings-section">
        <h2>Command Center</h2>
        <div className="settings-grid">
          <label>
            <span>Target branch</span>
            <input
              value={config.ship.target_branch}
              onChange={(e) => setConfig({ ...config, ship: { ...config.ship, target_branch: e.target.value } })}
            />
          </label>
          <label>
            <span>Dashboard port</span>
            <input
              type="number"
              value={config.ship.web_port}
              onChange={(e) => setConfig({ ...config, ship: { ...config.ship, web_port: Number(e.target.value) } })}
            />
          </label>
          <label>
            <span>Test runner</span>
            <input
              value={config.quality_gate.tests.runner}
              onChange={(e) =>
                setConfig({
                  ...config,
                  quality_gate: { ...config.quality_gate, tests: { runner: e.target.value } },
                })
              }
            />
          </label>
          <label>
            <span>PR provider</span>
            <select
              value={config.remote.provider}
              onChange={(e) => setConfig({ ...config, remote: { ...config.remote, provider: e.target.value } })}
            >
              <option value="azure_devops">Azure DevOps</option>
              <option value="github">GitHub</option>
            </select>
          </label>
          <label>
            <span>AzDO org</span>
            <input
              value={az.org}
              onChange={(e) =>
                setConfig({
                  ...config,
                  remote: {
                    ...config.remote,
                    azure_devops: { ...az, org: e.target.value },
                  },
                })
              }
            />
          </label>
          <label>
            <span>AzDO project</span>
            <input
              value={az.project}
              onChange={(e) =>
                setConfig({
                  ...config,
                  remote: {
                    ...config.remote,
                    azure_devops: { ...az, project: e.target.value },
                  },
                })
              }
            />
          </label>
          <label>
            <span>AzDO repo ID</span>
            <input
              value={az.repo_id}
              onChange={(e) =>
                setConfig({
                  ...config,
                  remote: {
                    ...config.remote,
                    azure_devops: { ...az, repo_id: e.target.value },
                  },
                })
              }
            />
          </label>
          <label>
            <span>AzDO token env</span>
            <input
              value={az.token_env}
              onChange={(e) =>
                setConfig({
                  ...config,
                  remote: {
                    ...config.remote,
                    azure_devops: { ...az, token_env: e.target.value },
                  },
                })
              }
            />
          </label>
        </div>

        <div className="page-actions">
          <button type="button" className="btn primary" disabled={!!busy} onClick={save}>
            {busy === 'save' ? 'Saving…' : 'Save settings'}
          </button>
        </div>
      </section>
    </div>
  );
}
