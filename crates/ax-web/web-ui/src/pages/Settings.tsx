import { useEffect, useState, type ReactNode } from 'react';

import AgentsSettingsSection from '../components/AgentsSettingsSection';
import EmbedSettingsSection from '../components/EmbedSettingsSection';
import PluginsSettingsSection from '../components/PluginsSettingsSection';
import SharingSettingsSection from '../components/SharingSettingsSection';
import McpTraceLive from '../components/McpTraceLive';
import { BusyLabel } from '../components/ui/PageLayout';
import ThemeChooser from '../components/ThemeChooser';
import { usePageContext } from '../context/UiContext';
import { DEFAULT_SONAR_CONFIG } from '../lib/sonarGuide';
import { loadThemeId } from '../lib/themes';
import { TIMEZONE_OPTIONS, browserTimeZone } from '../lib/timeZone';
import { fetchShipConfig, saveShipConfig, type ShipConfig } from '../shipApi';

const DEFAULT_CONFIG: ShipConfig = {
  ship: { target_branch: 'main', web_port: 7070, git_root: '', git_roots: [] },
  quality_gate: {
    steps: ['index', 'tia', 'tests', 'sonar', 'policy'],
    tests: { runner: 'cargo test' },
    index_mode: 'incremental',
  },
  remote: { provider: 'azure_devops' },
  sonar: { ...DEFAULT_SONAR_CONFIG },
  ui: {
    show_savings: true,
    show_agent_terminal: true,
    verbose_mcp: false,
    timezone: '',
  },
  reviewers: {},
};

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

export default function SettingsPage() {
  const [config, setConfig] = useState<ShipConfig>(DEFAULT_CONFIG);
  const [configLoaded, setConfigLoaded] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [themeId, setThemeId] = useState(loadThemeId);

  usePageContext('Settings', 'Command Center · pipeline & agents');

  useEffect(() => {
    let cancelled = false;

    async function load() {
      try {
        const d = await fetchShipConfig();
        if (cancelled) return;
        setConfig({
          ...DEFAULT_CONFIG,
          ...d.config,
          ship: { ...DEFAULT_CONFIG.ship, ...d.config.ship },
          ui: { ...DEFAULT_CONFIG.ui, ...d.config.ui },
          quality_gate: { ...DEFAULT_CONFIG.quality_gate, ...d.config.quality_gate },
          sonar: { ...DEFAULT_CONFIG.sonar, ...d.config.sonar },
        });
        setConfigLoaded(true);
      } catch (e) {
        if (!cancelled) {
          setErr(String(e));
          setConfigLoaded(true);
        }
      }
    }

    load();
    return () => {
      cancelled = true;
    };
  }, []);

  async function setUiSavings(show_savings: boolean) {
    const next = {
      ...config,
      ui: { ...(config.ui ?? {}), show_savings },
    };
    setConfig(next);
    setErr(null);
    setMsg(null);
    try {
      await saveShipConfig(next);
      window.dispatchEvent(
        new CustomEvent('ax-ship-config-updated', { detail: { show_savings } }),
      );
      setMsg(show_savings ? 'Savings page enabled in sidebar' : 'Savings page hidden');
    } catch (e) {
      setConfig((c) => ({
        ...c,
        ui: { ...(c.ui ?? {}), show_savings: !show_savings },
      }));
      setErr(String(e));
    }
  }

  async function setUiVerboseMcp(verbose_mcp: boolean) {
    const next = {
      ...config,
      ui: { ...(config.ui ?? {}), verbose_mcp },
    };
    setConfig(next);
    setErr(null);
    setMsg(null);
    try {
      await saveShipConfig(next);
      window.dispatchEvent(
        new CustomEvent('ax-ship-config-updated', { detail: { verbose_mcp } }),
      );
      setMsg(
        verbose_mcp
          ? 'Verbose MCP logging enabled — reconnect ax MCP, then open Logging from the status bar'
          : 'Verbose MCP logging disabled',
      );
    } catch (e) {
      setConfig((c) => ({
        ...c,
        ui: { ...(c.ui ?? {}), verbose_mcp: !verbose_mcp },
      }));
      setErr(String(e));
    }
  }

  async function setUiTimezone(timezone: string) {
    const prev = config.ui?.timezone ?? '';
    const next = {
      ...config,
      ui: { ...(config.ui ?? {}), timezone },
    };
    setConfig(next);
    setErr(null);
    setMsg(null);
    try {
      await saveShipConfig(next);
      window.dispatchEvent(
        new CustomEvent('ax-ship-config-updated', { detail: { timezone } }),
      );
      const label =
        !timezone || timezone === 'local'
          ? `Browser local (${browserTimeZone()})`
          : timezone;
      setMsg(`Logging timezone set to ${label}`);
    } catch (e) {
      setConfig((c) => ({
        ...c,
        ui: { ...(c.ui ?? {}), timezone: prev },
      }));
      setErr(String(e));
    }
  }

  async function save(nextConfig: ShipConfig = config) {
    setBusy('save');
    setErr(null);
    setMsg(null);
    try {
      await saveShipConfig(nextConfig);
      setConfig(nextConfig);
      setMsg('Settings saved to .ax/ship.toml');
    } catch (e) {
      setErr(String(e));
      throw e;
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

  const commandCenterLive =
    typeof window !== 'undefined' &&
    (window.location.port === String(config.ship.web_port) ||
      (!window.location.port && config.ship.web_port === 80));

  return (
    <div className="page settings-page">
      <header className="settings-hero">
        <h1 className="settings-hero-title">Settings</h1>
        <p className="settings-hero-sub">
          Command Center pipeline, pull requests, and agents — stored in <code>.ax/ship.toml</code>.
          SonarQube has its own page in the sidebar.
        </p>
      </header>

      {msg && <div className="settings-toast settings-toast--ok">{msg}</div>}
      {err && <div className="settings-toast settings-toast--err">{err}</div>}

      <div className="settings-stack">
        <AgentsSettingsSection />

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
                disabled={!configLoaded || !!busy}
                onChange={(e) =>
                  setConfig({ ...config, ship: { ...config.ship, target_branch: e.target.value } })
                }
              />
            </SettingRow>

            <SettingRow
              title="Git repositories"
              description="Subfolders with .git — auto-discovered on startup. Evaluate diffs all listed repos."
            >
              <textarea
                className="settings-input settings-input--mono"
                rows={Math.min(8, Math.max(3, (config.ship.git_roots?.length ?? 0) + 1))}
                readOnly
                value={(config.ship.git_roots ?? []).join('\n')}
                placeholder="Auto-discovered when you open Settings or start ax web"
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
                disabled={!!busy}
                onChange={(e) =>
                  setConfig({
                    ...config,
                    quality_gate: { ...config.quality_gate, tests: { runner: e.target.value } },
                  })
                }
              />
            </SettingRow>

            <SettingRow title="Index scope" description="Incremental syncs dirty files; full re-indexes the entire codebase.">
              <select
                className="settings-select"
                value={config.quality_gate.index_mode ?? 'incremental'}
                disabled={!!busy}
                onChange={(e) =>
                  setConfig({
                    ...config,
                    quality_gate: { ...config.quality_gate, index_mode: e.target.value },
                  })
                }
              >
                <option value="incremental">Incremental (sync)</option>
                <option value="full">Full codebase</option>
              </select>
            </SettingRow>

            <div className="settings-divider" />
            <div className="settings-subsection-label">Interface</div>

            <SettingRow
              title="Theme"
              description="Accent color and palette for the Command Center UI."
            >
              <ThemeChooser activeId={themeId} onSelect={setThemeId} />
            </SettingRow>

            <SettingRow
              title="Show Savings page"
              description="Measured context-token and dollar savings from ax MCP graph queries."
            >
              <Toggle
                label="Show Savings page"
                checked={config.ui?.show_savings ?? config.ui?.show_tokens ?? true}
                onChange={setUiSavings}
              />
            </SettingRow>

            <SettingRow
              title="Verbose MCP logging"
              description="Record inbound args, enrichment, outbound MCP payloads, and v4 domain events (plugin/lsp/ship-ci/share/workspace/embed/action) to <project>/.ax/mcp-verbose-YYYY-MM-DD.log and stream them in Logging. Off by default — never alters tool responses."
            >
              <Toggle
                label="Verbose MCP logging"
                checked={config.ui?.verbose_mcp ?? false}
                onChange={setUiVerboseMcp}
              />
            </SettingRow>

            <SettingRow
              title="Timezone"
              description={`Display Logging Date/time in this zone and rotate daily log files at midnight here. Line timestamps in files stay UTC. Browser local is currently ${browserTimeZone()}.`}
            >
              <select
                className="settings-select"
                value={config.ui?.timezone ?? ''}
                disabled={!!busy || !configLoaded}
                aria-label="Timezone for Logging timestamps"
                onChange={(e) => void setUiTimezone(e.target.value)}
              >
                {TIMEZONE_OPTIONS.map((opt) => (
                  <option key={opt.value || 'local'} value={opt.value}>
                    {opt.value === ''
                      ? `${opt.label} (${browserTimeZone()})`
                      : opt.label}
                  </option>
                ))}
                {config.ui?.timezone &&
                  !TIMEZONE_OPTIONS.some((o) => o.value === config.ui?.timezone) && (
                    <option value={config.ui.timezone}>{config.ui.timezone}</option>
                  )}
              </select>
            </SettingRow>

            <div className="settings-divider" />
            <div className="settings-subsection-label">Pull requests</div>

            <SettingRow title="PR provider" description="Where ship opens and updates pull requests.">
              <select
                className="settings-select"
                value={config.remote.provider}
                disabled={!!busy}
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
                    disabled={!!busy}
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
                    disabled={!!busy}
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
                    disabled={!!busy}
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
                    disabled={!!busy}
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
            <button type="button" className="btn primary" disabled={!!busy} onClick={() => void save()}>
              {busy === 'save' ? <BusyLabel label="Saving…" /> : 'Save settings'}
            </button>
          </div>
        </section>

        <section className="settings-card">
          <div className="settings-card-header">
            <h2>Sharing</h2>
            <p>LAN share session, token gate, and optional PWA install.</p>
          </div>
          <div className="settings-card-body">
            <SharingSettingsSection />
          </div>
        </section>

        <section className="settings-card">
          <div className="settings-card-header">
            <h2>Plugins</h2>
            <p>Process / WASM extractors discovered under <code>.ax/plugins</code>.</p>
          </div>
          <div className="settings-card-body">
            <PluginsSettingsSection />
          </div>
        </section>

        <section className="settings-card">
          <div className="settings-card-header">
            <h2>Embeddings</h2>
            <p>Memory embed backend (feature-hash or ONNX dense vectors).</p>
          </div>
          <div className="settings-card-body">
            <EmbedSettingsSection />
          </div>
        </section>

        <McpTraceLive verboseEnabled={config.ui?.verbose_mcp ?? false} />
      </div>
    </div>
  );
}
