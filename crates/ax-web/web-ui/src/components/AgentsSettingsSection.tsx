import { useCallback, useEffect, useState } from 'react';

import {
  createAgentProfile,
  ensureAgentReady,
  fetchAgentStatus,
  isCliReady,
  markProfileAuthenticated,
  runnableAgents,
  saveAgentConfig,
  streamAgentCliInstall,
  streamAgentInstall,
  streamProfileAuth,
  type AgentTargetStatus,
  type AgentsConfig,
} from '../agentApi';
import ProfileEditor from './agent/ProfileEditor';
import ModalShell from './ModalShell';

function SettingRow({ title, description, children }: { title: string; description?: string; children: React.ReactNode }) {
  return (
    <div className="settings-row">
      <div className="settings-row-label">
        <span className="settings-row-title">{title}</span>
        {description && <span className="settings-row-desc">{description}</span>}
      </div>
      <div className="settings-row-control">{children}</div>
    </div>
  );
}

function StatusPill({ label, tone }: { label: string; tone: 'ok' | 'warn' | 'muted' }) {
  return <span className={`settings-status-pill settings-status-pill--${tone}`}>{label}</span>;
}

function LogConsole({ lines }: { lines: string[] }) {
  const ref = useCallback((el: HTMLPreElement | null) => {
    if (el) el.scrollTop = el.scrollHeight;
  }, []);
  if (!lines.length) return null;
  return (
    <div className="settings-log-panel">
      <div className="settings-log-header"><span>Log</span></div>
      <pre ref={ref} className="settings-log-body">{lines.join('\n')}</pre>
    </div>
  );
}

function targetCliLabel(t: AgentTargetStatus): { label: string; tone: 'ok' | 'warn' | 'muted' } {
  if (isCliReady(t)) return { label: 'CLI ready', tone: 'ok' };
  if (t.runnable === false) return { label: 'MCP only', tone: 'muted' };
  if (t.data_dir_detected || (t.detected && !isCliReady(t))) return { label: 'Config detected', tone: 'warn' };
  if (t.cli_installable) return { label: 'CLI not installed', tone: 'muted' };
  return { label: 'Manual install', tone: 'muted' };
}

const PROFILE_AGENT_LABELS: Record<string, string> = {
  claude: 'Claude Code',
  cursor: 'Cursor',
  codex: 'Codex CLI',
  gemini: 'Gemini CLI',
  opencode: 'opencode',
  kiro: 'Kiro',
  builtin: 'Built-in ax',
};

export default function AgentsSettingsSection() {
  const [targets, setTargets] = useState<AgentTargetStatus[]>([]);
  const [config, setConfig] = useState<AgentsConfig>({
    enabled_targets: [],
    terminal_mode: 'auto',
    active_profile: {},
    profiles: {},
  });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [log, setLog] = useState<string[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [readonly, setReadonly] = useState(false);
  const [error, setError] = useState('');
  const [okMsg, setOkMsg] = useState('');
  const [newProfileAgent, setNewProfileAgent] = useState('claude');
  const [newProfileId, setNewProfileId] = useState('');
  const [newProfileLabel, setNewProfileLabel] = useState('');
  const [newProfileProvider, setNewProfileProvider] = useState('');
  const [newProfileKeyEnv, setNewProfileKeyEnv] = useState('');
  const [newProfileModel, setNewProfileModel] = useState('');
  const [addProfileOpen, setAddProfileOpen] = useState(false);

  const load = useCallback(async () => {
    const data = await fetchAgentStatus();
    setTargets(data.targets ?? []);
    if (data.config) setConfig(data.config);
    setReadonly(!!data.readonly);
    setLoaded(true);
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  function toggleTarget(id: string) {
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  }

  async function saveConfig(next: AgentsConfig): Promise<boolean> {
    if (readonly) {
      setError('Read-only mode — changes cannot be saved');
      return false;
    }
    if (!loaded) return false;
    const prev = config;
    setConfig(next);
    setError('');
    const res = await saveAgentConfig(next);
    if (!res.ok) {
      setConfig(prev);
      setError(res.error ?? 'Failed to save settings');
      return false;
    }
    setOkMsg('Settings saved');
    return true;
  }

  async function doInstallMcp() {
    const list = [...selected];
    if (!list.length) return;
    setBusy('install-mcp');
    setLog([]);
    setError('');
    await streamAgentInstall(list, (ev) => {
      if (ev.type === 'line') setLog((l) => [...l, ev.text]);
      if (ev.type === 'done') {
        setBusy(null);
        if (ev.ok === false) setError(ev.error ?? 'MCP install failed');
        else setOkMsg('MCP wired');
        void load();
      }
    });
  }

  async function doInstallCli() {
    const list = [...selected].filter((id) => {
      const t = targets.find((x) => x.id === id);
      return t?.cli_installable && !t.cli_on_path;
    });
    if (!list.length) return;
    setBusy('install-cli');
    setLog([]);
    setError('');
    await streamAgentCliInstall(list, (ev) => {
      if (ev.type === 'line') setLog((l) => [...l, ev.text]);
      if (ev.type === 'done') {
        setBusy(null);
        if (ev.ok === false) setError(ev.error ?? 'CLI install failed');
        else setOkMsg('CLI install finished');
        void load();
      }
    });
  }

  async function doUninstall() {
    const list = [...selected];
    if (!list.length) return;
    setBusy('uninstall');
    setError('');
    const res = await fetch('/api/agent/uninstall', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ targets: list }),
    }).then((r) => r.json());
    setBusy(null);
    if (res.ok) {
      setOkMsg('MCP removed');
      void load();
    } else {
      setError(res.error ?? 'Uninstall failed');
    }
  }

  const selectedTargets = targets.filter((t) => selected.has(t.id));
  const canInstallCli = selectedTargets.some((t) => t.cli_installable && !t.cli_on_path);

  async function addProfile() {
    if (!newProfileId.trim() || !newProfileLabel.trim()) {
      setError('Profile id and label are required');
      return;
    }
    if (readonly) {
      setError('Read-only mode — changes cannot be saved');
      return;
    }
    setBusy('profile');
    setError('');
    const res = await createAgentProfile({
      agent: newProfileAgent,
      id: newProfileId.trim(),
      label: newProfileLabel.trim(),
      provider: newProfileProvider.trim() || undefined,
      key_env: newProfileKeyEnv.trim() || undefined,
      model: newProfileModel.trim() || undefined,
    });
    setBusy(null);
    if (res.ok) {
      setNewProfileId('');
      setNewProfileLabel('');
      setNewProfileProvider('');
      setNewProfileKeyEnv('');
      setNewProfileModel('');
      setAddProfileOpen(false);
      setOkMsg('Profile created');
      void load();
    } else {
      setError(res.error ?? 'Failed to create profile');
    }
  }

  async function authProfile(agent: string, id: string) {
    setBusy(`auth-${agent}-${id}`);
    setLog([]);
    setError('');
    setLog(['Preparing authentication…']);
    const ready = await ensureAgentReady(agent, (text) => setLog((l) => [...l, text]));
    if (!ready.ok) {
      setError(ready.error ?? 'Could not install agent CLI');
    }
    await streamProfileAuth(agent, id, (ev) => {
      if (ev.type === 'line') setLog((l) => [...l, ev.text]);
      if (ev.type === 'done') {
        setBusy(null);
        if (ev.ok === false) {
          setError(ev.error ?? 'Authentication failed — log in manually, then click Mark authenticated');
        } else if (ev.manual) {
          setError('Complete login manually, then click Mark authenticated');
        } else {
          setOkMsg('Authentication finished');
        }
        void load();
      }
    });
  }

  async function markAuthenticated(agent: string, id: string) {
    setBusy(`mark-${agent}-${id}`);
    setError('');
    const res = await markProfileAuthenticated(agent, id);
    setBusy(null);
    if (res.ok) {
      setOkMsg('Profile marked authenticated');
      void load();
    } else {
      setError(res.error ?? 'Could not update profile');
    }
  }

  const runnable = runnableAgents(targets);
  const profileAgents = ['builtin', ...runnable.map((t) => t.id)];

  return (
    <section className="settings-card">
      <div className="settings-card-header">
        <h2>AI Agents</h2>
        <p>Install agent CLIs, wire ax MCP, choose terminal mode, and manage account profiles.</p>
      </div>
      <div className="settings-card-body">
        {readonly && (
          <div className="settings-toast settings-toast--err">
            Read-only mode — install, profile, and settings changes are disabled.
          </div>
        )}
        {error && <div className="settings-toast settings-toast--err">{error}</div>}
        {okMsg && <div className="settings-toast settings-toast--ok">{okMsg}</div>}

        <SettingRow title="Terminal mode" description="Auto uses external agent on desktop when detected; built-in works everywhere.">
          <select
            className="settings-input"
            value={config.terminal_mode}
            disabled={!loaded || readonly}
            onChange={(e) => void saveConfig({ ...config, terminal_mode: e.target.value })}
          >
            <option value="auto">Auto</option>
            <option value="builtin">Built-in only</option>
            <option value="external">External only</option>
          </select>
        </SettingRow>
        <SettingRow title="Preferred external agent" description="Used when terminal mode is auto or external.">
          <select
            className="settings-input"
            value={config.preferred_external ?? 'cursor'}
            disabled={!loaded || readonly}
            onChange={(e) => void saveConfig({ ...config, preferred_external: e.target.value })}
          >
            {runnable.map((t) => (
              <option key={t.id} value={t.id}>{t.display_name}</option>
            ))}
          </select>
        </SettingRow>

        <div className="settings-subsection-label">Agent targets</div>
        {targets.map((t) => {
          const cli = targetCliLabel(t);
          return (
            <div key={t.id} className="agent-target-row">
              <label className="settings-row settings-row--check">
                <input type="checkbox" checked={selected.has(t.id)} onChange={() => toggleTarget(t.id)} />
                <span className="settings-row-title">{t.display_name}</span>
                <StatusPill label={cli.label} tone={cli.tone} />
                <StatusPill label={t.configured ? 'MCP wired' : 'MCP not wired'} tone={t.configured ? 'ok' : 'warn'} />
              </label>
              {t.config_paths.length > 0 && (
                <div className="agent-target-paths">
                  {t.config_paths.map((p) => (
                    <code key={p} className="agent-path-chip">{p}</code>
                  ))}
                </div>
              )}
            </div>
          );
        })}
        <div className="settings-card-footer" style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          <button
            type="button"
            className="btn primary"
            disabled={!!busy || !canInstallCli || readonly}
            onClick={() => void doInstallCli()}
            title="Install the selected agent CLI (Claude, Codex, Cursor, etc.)"
          >
            Install CLI
          </button>
          <button type="button" className="btn btn-subtle" disabled={!!busy || selected.size === 0 || readonly} onClick={() => void doInstallMcp()}>
            Wire ax MCP
          </button>
          <button type="button" className="btn btn-subtle" disabled={!!busy || selected.size === 0 || readonly} onClick={() => void doUninstall()}>
            Remove ax MCP
          </button>
        </div>

        <div className="settings-subsection-label">Account profiles</div>
        <p className="settings-row-desc agent-profiles-intro">
          Profiles isolate accounts per agent. Use <strong>claude</strong> / <strong>cursor</strong> for separate logins
          (work vs personal). Use <strong>builtin</strong> for the built-in ax chat LLM (provider + API key env var).
          Switch profiles in the Agent terminal dropdown.
        </p>
        <div className="settings-card-footer" style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginBottom: 12 }}>
          <button
            type="button"
            className="btn primary"
            disabled={!!busy || readonly}
            onClick={() => setAddProfileOpen(true)}
          >
            Add profile
          </button>
        </div>

        {addProfileOpen && (
          <ModalShell
            title="Add profile"
            subtitle="Create a new account profile for an agent. Profiles appear in the Agent terminal dropdown."
            onClose={() => setAddProfileOpen(false)}
            footer={
              <>
                <button type="button" className="btn btn-subtle" disabled={!!busy} onClick={() => setAddProfileOpen(false)}>
                  Cancel
                </button>
                <button type="button" className="btn primary" disabled={!!busy || readonly} onClick={() => void addProfile()}>
                  Add profile
                </button>
              </>
            }
          >
            <div className="ax-modal-form-stack">
              <div className="ax-modal-form-row">
                <select className="settings-input" value={newProfileAgent} disabled={readonly} onChange={(e) => setNewProfileAgent(e.target.value)}>
                  {profileAgents.map((id) => (
                    <option key={id} value={id}>{PROFILE_AGENT_LABELS[id] ?? targets.find((t) => t.id === id)?.display_name ?? id}</option>
                  ))}
                </select>
                <input className="settings-input" placeholder="Profile id (e.g. work)" value={newProfileId} disabled={readonly} onChange={(e) => setNewProfileId(e.target.value)} />
              </div>
              <input className="settings-input" placeholder="Display label (e.g. Work account)" value={newProfileLabel} disabled={readonly} onChange={(e) => setNewProfileLabel(e.target.value)} />
              {newProfileAgent === 'builtin' && (
                <>
                  <select className="settings-input" value={newProfileProvider} disabled={readonly} onChange={(e) => setNewProfileProvider(e.target.value)}>
                    <option value="">Provider (default)</option>
                    <option value="openai">OpenAI</option>
                    <option value="anthropic">Anthropic</option>
                    <option value="google">Google</option>
                    <option value="openrouter">OpenRouter</option>
                  </select>
                  <input className="settings-input" placeholder="API key env var (OPENAI_API_KEY)" value={newProfileKeyEnv} disabled={readonly} onChange={(e) => setNewProfileKeyEnv(e.target.value)} />
                  <input className="settings-input" placeholder="Model (gpt-4o-mini)" value={newProfileModel} disabled={readonly} onChange={(e) => setNewProfileModel(e.target.value)} />
                </>
              )}
            </div>
          </ModalShell>
        )}

        {profileAgents.map((agent) => {
          const list = config.profiles?.[agent] ?? [];
          return (
            <div key={agent} className="agent-profile-group">
              <div className="agent-profile-group-header">
                <span className="agent-profile-group-title">{PROFILE_AGENT_LABELS[agent] ?? targets.find((t) => t.id === agent)?.display_name ?? agent}</span>
                <span className="agent-profile-count">{list.length} profile{list.length === 1 ? '' : 's'}</span>
              </div>
              {list.length === 0 ? (
                <p className="agent-profile-empty">No profiles yet — click Add profile above.</p>
              ) : (
                <div className="agent-profile-list">
                  {list.map((p) => (
                    <div key={p.id} className="agent-profile-card">
                      <ProfileEditor
                        agent={agent}
                        profile={p}
                        readonly={readonly}
                        onSaved={() => {
                          setOkMsg('Profile saved');
                          void load();
                        }}
                        onError={setError}
                      />
                      {agent !== 'builtin' && p.auth_status !== 'authenticated' && (
                        <div className="agent-profile-actions">
                          <button type="button" className="btn btn-subtle btn-sm" onClick={() => void authProfile(agent, p.id)} disabled={!!busy || readonly}>
                            Authenticate
                          </button>
                          <button type="button" className="btn btn-subtle btn-sm" onClick={() => void markAuthenticated(agent, p.id)} disabled={!!busy || readonly} title="Use after logging in manually via the agent CLI">
                            Mark authenticated
                          </button>
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          );
        })}

        <LogConsole lines={log} />
      </div>
    </section>
  );
}
