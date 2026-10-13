import useMobile from '../hooks/useMobile';
import ModalShell from '../components/ModalShell';
import AccessSettingsSection from '../components/AccessSettingsSection';
import SecuritySettingsSection from '../components/SecuritySettingsSection';
import { useEffect, useState, type ReactNode } from 'react';

import EmbedSettingsSection from '../components/EmbedSettingsSection';
import RemoteMcpSettingsSection from '../components/RemoteMcpSettingsSection';
import OkfSettingsSection from '../components/OkfSettingsSection';
import PluginsSettingsSection from '../components/PluginsSettingsSection';
import SharingSettingsSection from '../components/SharingSettingsSection';
import VaultMountCard from '../components/VaultMountCard';
import VaultFoldersCard from '../components/VaultFoldersCard';
import IdeInstallCard from '../components/IdeInstallCard';
import PolicyShareSettingsSection from '../components/PolicyShareSettingsSection';
import PolicySyncSettingsSection from '../components/PolicySyncSettingsSection';
import { BusyLabel, PageHero } from '../components/ui/PageLayout';
import ThemeChooser from '../components/ThemeChooser';
import { usePageContext } from '../context/UiContext';
import { DEFAULT_SONAR_CONFIG } from '../lib/sonarGuide';
import { loadThemeId, THEME_CHANGED } from '../lib/themes';
import { SETTINGS_GROUPS, readSettingsSection, type SettingsSectionId } from '../lib/settingsNavigation';
import { TIMEZONE_OPTIONS, browserTimeZone } from '../lib/timeZone';
import SettingsSelect from '../components/ui/SettingsSelect';
import {
  fetchReviewLanguage,
  fetchShipConfig,
  saveReviewLanguage,
  saveShipConfig,
  type ReviewLanguageChoice,
  type ShipConfig,
} from '../shipApi';

const REVIEW_LANGUAGES: ReviewLanguageChoice[] = [
  { code: 'en', name: 'English' },
  { code: 'nl', name: 'Nederlands' },
  { code: 'de', name: 'Deutsch' },
  { code: 'fr', name: 'Français' },
  { code: 'es', name: 'Español' },
  { code: 'pt', name: 'Português' },
  { code: 'it', name: 'Italiano' },
  { code: 'pl', name: 'Polski' },
  { code: 'sv', name: 'Svenska' },
  { code: 'da', name: 'Dansk' },
  { code: 'tr', name: 'Türkçe' },
];

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
    verbose_mcp: true,
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
  const mobile = useMobile();
  const [navigationOpen, setNavigationOpen] = useState(false);
  const [config, setConfig] = useState<ShipConfig>(DEFAULT_CONFIG);
  const [configLoaded, setConfigLoaded] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [themeId, setThemeId] = useState(loadThemeId);
  const [section, setSection] = useState<SettingsSectionId>(readSettingsSection);
  const [settingsFilter, setSettingsFilter] = useState('');
  const [reviewLanguage, setReviewLanguage] = useState('en');
  const [reviewLanguages, setReviewLanguages] = useState<ReviewLanguageChoice[]>(REVIEW_LANGUAGES);
  const [contextBudget, setContextBudget] = useState(8_000);
  const [contextBudgetSet, setContextBudgetSet] = useState(false);

  usePageContext('Settings', 'Command Center · pipeline & agents');

  useEffect(() => {
    const url = new URL(window.location.href);
    url.searchParams.set('section', section);
    window.history.replaceState(window.history.state, '', url);
  }, [section]);

  useEffect(() => {
    const onTheme = () => setThemeId(loadThemeId());
    window.addEventListener(THEME_CHANGED, onTheme);
    return () => window.removeEventListener(THEME_CHANGED, onTheme);
  }, []);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      try {
        const d = await fetchShipConfig();
        if (cancelled) return;
        try {
          const language = await fetchReviewLanguage();
          if (!cancelled) {
            setReviewLanguage(language.code);
            if (language.languages.length > 0) setReviewLanguages(language.languages);
          }
        } catch {
          // The dropdown already lists every language. A missing endpoint must not blank it.
        }
        setConfig({
          ...DEFAULT_CONFIG,
          ...d.config,
          ship: { ...DEFAULT_CONFIG.ship, ...d.config.ship },
          ui: { ...DEFAULT_CONFIG.ui, ...d.config.ui },
          quality_gate: { ...DEFAULT_CONFIG.quality_gate, ...d.config.quality_gate },
          sonar: { ...DEFAULT_CONFIG.sonar, ...d.config.sonar },
        });
        setConfigLoaded(true);
        try {
          const budget = await fetch('/api/context-budget/').then((res) => res.json()) as { tokens?: number | null };
          if (!cancelled && typeof budget.tokens === 'number') {
            setContextBudget(budget.tokens);
            setContextBudgetSet(true);
          }
        } catch {
          // The slider still saves a project override when the current value cannot be read.
        }
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

  async function setReviewCommentLanguage(code: string) {
    const prev = reviewLanguage;
    setReviewLanguage(code);
    setErr(null);
    setMsg(null);
    try {
      const saved = await saveReviewLanguage(code);
      const label = reviewLanguages.find((opt) => opt.code === saved.code)?.name ?? saved.name;
      setMsg(`Review comments set to ${label}`);
    } catch (e) {
      setReviewLanguage(prev);
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

  const settingsNavigation = (
        <nav className="settings-navigation" aria-label="Settings navigation">
          <label className="settings-navigation-filter">
            <i className="codicon codicon-search" aria-hidden="true" />
            <input aria-label="Filter settings" placeholder="Filter settings" value={settingsFilter}
              onChange={(event) => setSettingsFilter(event.target.value)} />
          </label>
          {SETTINGS_GROUPS.map((group) => {
            const items = group.items.filter((item) => `${group.label} ${item.label}`.toLowerCase().includes(settingsFilter.trim().toLowerCase()));
            return items.length > 0 && <div className="settings-navigation-group" key={group.label}>
              <h2>{group.label}</h2>
              {items.map((item) => <button key={item.id} type="button"
                className={`settings-navigation-item${section === item.id ? ' active' : ''}`}
                aria-current={section === item.id ? 'page' : undefined} aria-controls={`settings-panel-${item.id}`}
                onClick={() => {setSection(item.id);setNavigationOpen(false);}}>
                <i className={`codicon codicon-${item.icon}`} aria-hidden="true" />{item.label}
              </button>)}
            </div>;
          })}
          {!SETTINGS_GROUPS.some((group) => group.items.some((item) => `${group.label} ${item.label}`.toLowerCase().includes(settingsFilter.trim().toLowerCase()))) &&
            <p className="settings-navigation-empty" role="status">No matching settings</p>}
        </nav>
  );

  return (
    <div className="page settings-page settings-hub">
      <PageHero title="Settings" subtitle="Workspace configuration, connections, and appearance." />

      {msg && <div className="settings-toast settings-toast--ok">{msg}</div>}
      {err && <div className="settings-toast settings-toast--err">{err}</div>}

      <div className="settings-hub-layout">
        {mobile ? <div className="mobile-settings-selector">
          <button type="button" className="btn" aria-label="Choose settings section" aria-haspopup="dialog" aria-expanded={navigationOpen} onClick={() => setNavigationOpen(true)}>
            {SETTINGS_GROUPS.flatMap(group => group.items).find(item => item.id === section)?.label} <span aria-hidden="true">⌄</span>
          </button>
          {navigationOpen && <ModalShell title="Settings sections" onClose={() => setNavigationOpen(false)}>{settingsNavigation}</ModalShell>}
        </div> : settingsNavigation}
        <div className="settings-hub-content">
        <AccessSettingsSection section={section} />
        {section === 'security' && <SecuritySettingsSection />}
        <div id="settings-panel-appearance" hidden={section !== 'appearance'}>
          <section className="settings-card">
            <div className="settings-card-header"><h2>Appearance</h2><p>Theme and display preferences.</p></div>
            <div className="settings-card-body">

            <div className="settings-theme-block">
              <span className="settings-row-title">Theme</span>
              <span className="settings-row-desc">
                Choose Light or Dark, then an accent color below. Appearance changes are saved automatically in this browser.
              </span>
              <ThemeChooser activeId={themeId} onSelect={setThemeId} />
            </div>

            <SettingRow
              title="MCP Output"
              description="Ax automatically optimizes MCP responses for AI agents using compact Markdown, structured metadata and on-demand context retrieval. No manual format selection is required."
            >
              <span className="settings-row-title">Smart Output</span>
            </SettingRow>

            <SettingRow
              title="Context budget"
              description="Optional preflight blocks stop once this many tokens are used. Required rules still come through. The value is stored in this project's ax.json."
            >
              <label className="settings-budget">
                <input
                  type="range"
                  min={1000}
                  max={64000}
                  step={1000}
                  value={contextBudget}
                  aria-label="Context budget tokens"
                  disabled={!!busy || !configLoaded}
                  onChange={(event) => {
                    const tokens = Number(event.target.value);
                    setContextBudget(tokens);
                    setContextBudgetSet(true);
                    void fetch('/api/context-budget/', {
                      method: 'PUT',
                      headers: { 'Content-Type': 'application/json' },
                      body: JSON.stringify({ tokens }),
                    }).then((res) => {
                      if (!res.ok) setErr('Could not save the context budget');
                      else setMsg(`Context budget set to ${tokens.toLocaleString()} tokens`);
                    }).catch(() => setErr('Could not save the context budget'));
                  }}
                />
                <span>{contextBudgetSet ? `${contextBudget.toLocaleString()} tokens` : 'Not set'}</span>
              </label>
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
              title="Timezone"
              description={`Display Logging Date/time in this zone and rotate daily log files at midnight here. Line timestamps in files stay UTC. Browser local is currently ${browserTimeZone()}.`}
            >
              <SettingsSelect
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
              </SettingsSelect>
            </SettingRow>


            </div>
          </section>
        </div>
        <div id="settings-panel-agents" hidden={section !== 'agents'}><IdeInstallCard /></div>
        <div id="settings-panel-command-center" hidden={section !== 'command-center'}>
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
              <SettingsSelect
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
              </SettingsSelect>
            </SettingRow>

            <div className="settings-divider" />
            <div className="settings-subsection-label">Pull requests</div>

            <SettingRow
              title="Review comment language"
              description="Language of comments the agent posts on a pull request. Chat stays English."
            >
              <SettingsSelect
                className="settings-select"
                value={reviewLanguage}
                disabled={!!busy || !configLoaded}
                aria-label="Review comment language"
                onChange={(e) => void setReviewCommentLanguage(e.target.value)}
              >
                {reviewLanguages.map((opt) => (
                  <option key={opt.code} value={opt.code}>
                    {opt.name}
                  </option>
                ))}
              </SettingsSelect>
            </SettingRow>

            <SettingRow title="PR provider" description="Where ship opens and updates pull requests.">
              <SettingsSelect
                className="settings-select"
                value={config.remote.provider}
                disabled={!!busy}
                onChange={(e) => setConfig({ ...config, remote: { ...config.remote, provider: e.target.value } })}
              >
                <option value="azure_devops">Azure DevOps</option>
                <option value="github">GitHub</option>
              </SettingsSelect>
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

        </div>
        <div id="settings-panel-sharing" hidden={section !== 'sharing'}>
        <section className="settings-card">
          <div className="settings-card-header">
            <h2>Sharing</h2>
            <p>LAN share session, token gate, and optional PWA install.</p>
          </div>
          <div className="settings-card-body">
            <SharingSettingsSection />
          </div>
        </section>

        </div>
        <div id="settings-panel-replication" hidden={section !== 'replication'}>
          <section className="settings-card">
            <div className="settings-card-header"><h2>Policy sync</h2><p>Configure synchronization between Ax instances.</p></div>
            <div className="settings-card-body"><PolicySyncSettingsSection /></div>
          </section>
        </div>
        <div id="settings-panel-remote" hidden={section !== 'remote'}>
          <section className="settings-card">
            <div className="settings-card-header"><h2>Remote MCP</h2><p>Connect external agents to Ax.</p></div>
            <div className="settings-card-body"><RemoteMcpSettingsSection /></div>
          </section>
        </div>
        <div id="settings-panel-vault" hidden={section !== 'vault'}>
        <section className="settings-card">
          <div className="settings-card-header">
            <h2>Vault connection</h2>
            <p>Connect rules, skills, and memories as a WebDAV drive (for Obsidian or any editor).</p>
          </div>
          <div className="settings-card-body">
            <VaultMountCard />
            <VaultFoldersCard />
          </div>
        </section>

        </div>
        <div id="settings-panel-export" hidden={section !== 'export'}>
        <section className="settings-card">
          <div className="settings-card-header">
            <h2>Open Knowledge Format (OKF)</h2>
            <p>
              Generate a portable Markdown OKF concept bundle from the indexed graph (
              <code>ax export okf</code>).
            </p>
          </div>
          <div className="settings-card-body">
            <OkfSettingsSection />
          </div>
        </section>

        </div>
        <div id="settings-panel-plugins" hidden={section !== 'plugins'}>
        <section className="settings-card">
          <div className="settings-card-header">
            <h2>Plugins</h2>
            <p>Process / WASM extractors discovered under <code>.ax/plugins</code>.</p>
          </div>
          <div className="settings-card-body">
            <PluginsSettingsSection />
          </div>
        </section>

        </div>
        <div id="settings-panel-embeddings" hidden={section !== 'embeddings'}>
        <section className="settings-card">
          <div className="settings-card-header">
            <h2>Embeddings</h2>
            <p>Memory embed backend (feature-hash or ONNX dense vectors).</p>
          </div>
          <div className="settings-card-body">
            <EmbedSettingsSection />
          </div>
        </section>

        </div>
        <div id="settings-panel-policy-sharing" hidden={section !== 'policy-sharing'}><PolicyShareSettingsSection /></div>
        </div>
      </div>
    </div>
  );
}
