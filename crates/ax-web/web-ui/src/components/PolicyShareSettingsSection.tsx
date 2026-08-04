import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';

import ModalShell from './ModalShell';
import { BusyLabel } from './ui/PageLayout';
import {
  AUTO_SYNC_OPTIONS,
  DEFAULT_SHARE_CONFIG,
  fetchMicrosoftAuthStatus,
  fetchShareConfig,
  fetchShareStatus,
  IMPORT_MODE_OPTIONS,
  pollMicrosoftDeviceFlow,
  runShareSync,
  saveShareConfig,
  signOutMicrosoft,
  startMicrosoftDeviceFlow,
  type DeviceFlowStart,
  type MicrosoftAuthStatus,
  type ShareConfig,
  type ShareConfigResponse,
  type ShareProvider,
  type ShareStatusResponse,
  type ShareSyncStatus,
} from '../shareApi';

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

function SettingRow({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
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

function ProviderIcon({ provider }: { provider: ShareProvider }) {
  if (provider === 'github') {
    return (
      <svg className="share-provider-icon" viewBox="0 0 24 24" aria-hidden="true">
        <path
          fill="currentColor"
          d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.395-.135-.345-.72-1.395-1.23-1.875-.42-.405-1.02-.705-.015-.72.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12Z"
        />
      </svg>
    );
  }
  return (
    <svg className="share-provider-icon" viewBox="0 0 24 24" aria-hidden="true">
      <path
        fill="currentColor"
        d="M12.5 2.5h8.25A1.25 1.25 0 0 1 22 3.75v8.25a1.25 1.25 0 0 1-1.25 1.25H12.5A1.25 1.25 0 0 1 11.25 13V3.75A1.25 1.25 0 0 1 12.5 2.5ZM3.75 2.5H12a1.25 1.25 0 0 1 1.25 1.25v8.25A1.25 1.25 0 0 1 12 13H3.75A1.25 1.25 0 0 1 2.5 11.75V3.75A1.25 1.25 0 0 1 3.75 2.5Zm0 11.25H12A1.25 1.25 0 0 1 13.25 15v8.25A1.25 1.25 0 0 1 12 24H3.75A1.25 1.25 0 0 1 2.5 22.75V15A1.25 1.25 0 0 1 3.75 13.75Zm8.75 0h8.25A1.25 1.25 0 0 1 22 15v8.25A1.25 1.25 0 0 1 20.75 24.75H12.5A1.25 1.25 0 0 1 11.25 23.5V15a1.25 1.25 0 0 1 1.25-1.25Z"
      />
    </svg>
  );
}

const HOSTED_TOOLS_BRIDGE_URL = 'https://bridge.hosted-tools.com/myprofile/settings';

function isHostedToolsGitlab(repoUrl: string): boolean {
  return /gitlab\.hosted-tools\.com/i.test(repoUrl.trim());
}

function formatSyncTime(ts?: number | null): string {
  if (!ts) return 'Never';
  const d = new Date(ts * 1000);
  if (Number.isNaN(d.getTime())) return 'Unknown';
  return d.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

function mergeConfig(raw: ShareConfig | ShareConfigResponse): ShareConfig {
  const { configPath: _p, scope: _s, ...rest } = raw as ShareConfigResponse;
  return {
    ...DEFAULT_SHARE_CONFIG,
    ...rest,
    content: { ...DEFAULT_SHARE_CONFIG.content, ...rest.content },
    onedrive: { ...DEFAULT_SHARE_CONFIG.onedrive, ...rest.onedrive },
    github: { ...DEFAULT_SHARE_CONFIG.github, ...rest.github },
  };
}

function MicrosoftSignInModal({
  flow,
  busy,
  error,
  onClose,
  onOpenUri,
}: {
  flow: DeviceFlowStart;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onOpenUri: () => void;
}) {
  return (
    <ModalShell
      title="Connect OneDrive"
      subtitle="Sign in with your Microsoft work account to sync shared policy."
      onClose={onClose}
      footer={
        <button type="button" className="btn btn-subtle" onClick={onClose}>
          Cancel
        </button>
      }
    >
      <p className="settings-row-desc" style={{ marginBottom: 12 }}>
        Browser opened — complete sign-in there. If it did not open, use the button below or enter the code manually.
      </p>
      <div className="share-device-code-block">
        <span className="share-device-code-label">Your code</span>
        <code className="share-device-code-value">{flow.userCode}</code>
      </div>
      <div className="settings-row" style={{ border: 'none', paddingInline: 0 }}>
        <div className="settings-row-label">
          <span className="settings-row-title">Verification URL</span>
          <span className="settings-row-desc mono">{flow.verificationUriComplete || flow.verificationUri}</span>
        </div>
        <div className="settings-row-control">
          <button type="button" className="btn btn-subtle" onClick={onOpenUri}>
            Open in browser
          </button>
        </div>
      </div>
      {busy && (
        <p className="settings-row-desc" style={{ marginTop: 12 }}>
          Waiting for sign-in…
        </p>
      )}
      {error && (
        <div className="settings-toast settings-toast--err" style={{ marginTop: 12 }}>
          {error}
        </div>
      )}
    </ModalShell>
  );
}

/** Settings → Remote policy share: GitHub or OneDrive Graph sync. */
export default function PolicyShareSettingsSection() {
  const [config, setConfig] = useState<ShareConfig>(DEFAULT_SHARE_CONFIG);
  const [configPath, setConfigPath] = useState('ax.json');
  const [status, setStatus] = useState<ShareStatusResponse | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [msModal, setMsModal] = useState<DeviceFlowStart | null>(null);
  const [msModalErr, setMsModalErr] = useState<string | null>(null);
  const [msPolling, setMsPolling] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const stopPolling = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
    setMsPolling(false);
  }, []);

  const refreshStatus = useCallback(async () => {
    const st = await fetchShareStatus();
    setStatus(st);
    return st;
  }, []);

  const loadAll = useCallback(async () => {
    setErr(null);
    try {
      const [cfg, st] = await Promise.all([fetchShareConfig(), fetchShareStatus()]);
      setConfig(mergeConfig(cfg));
      setConfigPath(cfg.configPath || 'ax.json');
      setStatus(st);
      setLoaded(true);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void loadAll();
    return () => stopPolling();
  }, [loadAll, stopPolling]);

  useEffect(() => {
    if (!msModal) return;
    const url = msModal.verificationUriComplete || msModal.verificationUri;
    if (url) {
      window.open(url, '_blank', 'noopener,noreferrer');
    }
  }, [msModal]);

  async function persist(next: ShareConfig, toast?: string) {
    setBusy('save');
    setErr(null);
    setMsg(null);
    try {
      const saved = await saveShareConfig(next);
      setConfig(mergeConfig(saved));
      setConfigPath(saved.configPath || 'ax.json');
      setMsg(toast ?? `Remote share settings saved to ${saved.configPath || 'ax.json'}`);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
      throw e;
    } finally {
      setBusy(null);
    }
  }

  function patch(partial: Partial<ShareConfig>) {
    const next = mergeConfig({ ...config, ...partial });
    setConfig(next);
    return next;
  }

  async function setProvider(provider: ShareProvider) {
    const next = patch({ provider });
    await persist(next, `Provider set to ${provider === 'github' ? 'GitHub' : 'OneDrive'}`);
  }

  async function saveCurrent() {
    await persist(config);
  }

  async function runSync() {
    setBusy('sync');
    setErr(null);
    setMsg(null);
    try {
      const syncStatus = await runShareSync('pull');
      setStatus((prev) =>
        prev
          ? { ...prev, sync: syncStatus, microsoft: prev.microsoft }
          : { sync: syncStatus, microsoft: { signedIn: false } },
      );
      setMsg(
        `Sync complete: +${syncStatus.rulesAdded} rules, +${syncStatus.skillsAdded} skills, pending ${syncStatus.rulesPending}/${syncStatus.skillsPending}`,
      );
      await refreshStatus();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
      await refreshStatus().catch(() => undefined);
    } finally {
      setBusy(null);
    }
  }

  function beginDevicePoll(intervalSec: number) {
    stopPolling();
    setMsPolling(true);
    const intervalMs = Math.max(5, intervalSec) * 1000;
    pollRef.current = setInterval(() => {
      void (async () => {
        try {
          const result = await pollMicrosoftDeviceFlow();
          setStatus((prev) =>
            prev
              ? { ...prev, microsoft: result.status }
              : { sync: emptySyncStatus(), microsoft: result.status },
          );
          if (result.complete) {
            stopPolling();
            setMsPolling(false);
            setMsModal(null);
            setMsModalErr(null);
            setBusy('sync');
            setErr(null);
            try {
              await saveShareConfig(config);
              const syncStatus = await runShareSync('pull');
              setStatus((prev) =>
                prev
                  ? { ...prev, sync: syncStatus, microsoft: result.status }
                  : { sync: syncStatus, microsoft: result.status },
              );
              setMsg(
                `Signed in as ${result.status.account ?? 'Microsoft account'} — policy synced (+${syncStatus.rulesAdded} rules, +${syncStatus.skillsAdded} skills).`,
              );
              await refreshStatus();
            } catch (e) {
              setErr(
                `Signed in, but sync failed: ${e instanceof Error ? e.message : String(e)}. Use Sync now to retry.`,
              );
              await refreshStatus().catch(() => undefined);
            } finally {
              setBusy(null);
            }
          }
        } catch (e) {
          setMsModalErr(e instanceof Error ? e.message : String(e));
          stopPolling();
          setMsPolling(false);
        }
      })();
    }, intervalMs);
  }

  async function startMicrosoftSignIn() {
    setBusy('ms-start');
    setMsModalErr(null);
    setErr(null);
    try {
      const flow = await startMicrosoftDeviceFlow();
      setMsModal(flow);
      beginDevicePoll(flow.interval);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  async function signOutMs() {
    setBusy('ms-out');
    setErr(null);
    setMsg(null);
    try {
      await signOutMicrosoft();
      const ms = await fetchMicrosoftAuthStatus();
      setStatus((prev) =>
        prev ? { ...prev, microsoft: ms } : { sync: emptySyncStatus(), microsoft: ms },
      );
      setMsg('Signed out of Microsoft');
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  function closeMsModal() {
    stopPolling();
    setMsPolling(false);
    setMsModal(null);
    setMsModalErr(null);
  }

  const microsoft = status?.microsoft ?? { signedIn: false };
  const sync = status?.sync ?? emptySyncStatus();
  const importModeHelp = IMPORT_MODE_OPTIONS.find((o) => o.value === config.importMode)?.description;

  return (
    <section className="settings-card">
      <div className="settings-card-header">
        <h2>Remote policy share</h2>
        <p>
          Per project — stored in <code>{configPath}</code>. Pull team rules, skills, and optional
          memory from GitHub or OneDrive (Microsoft Graph). Manage per workspace in Takumi
          Preferences or here in Command Center.
        </p>
      </div>

      <div className="settings-card-body">
        {msg && <div className="settings-toast settings-toast--ok">{msg}</div>}
        {err && <div className="settings-toast settings-toast--err">{err}</div>}

        <div className="settings-subsection-label">Provider</div>

        <div className="settings-row settings-row--stack">
          <div className="settings-row-label">
            <span className="settings-row-title">Remote source</span>
            <span className="settings-row-desc">Choose where team policy is hosted.</span>
          </div>
          <div className="settings-row-control">
            <div className="share-provider-chooser" role="radiogroup" aria-label="Share provider">
              {(['onedrive', 'github'] as ShareProvider[]).map((id) => {
                const active = config.provider === id;
                const label = id === 'github' ? 'GitHub' : 'OneDrive';
                return (
                  <button
                    key={id}
                    type="button"
                    role="radio"
                    aria-checked={active}
                    className={`share-provider-card${active ? ' share-provider-card--active' : ''}`}
                    disabled={!loaded || !!busy}
                    onClick={() => void setProvider(id)}
                  >
                    <ProviderIcon provider={id} />
                    <span className="share-provider-card-label">{label}</span>
                    {active && <span className="share-provider-card-check" aria-hidden="true">✓</span>}
                  </button>
                );
              })}
            </div>
          </div>
        </div>

        {config.provider === 'onedrive' && (
          <>
            <SettingRow
              title="OneDrive share URL"
              description="SharePoint folder containing .ax/policy/shared/ (and optional memory/shared.jsonl)."
            >
              <input
                className="settings-input settings-input--wide settings-input--mono"
                value={config.onedrive.shareUrl}
                disabled={!loaded || !!busy}
                onChange={(e) => patch({ onedrive: { ...config.onedrive, shareUrl: e.target.value } })}
                onBlur={() => void saveCurrent().catch(() => undefined)}
                placeholder={DEFAULT_SHARE_CONFIG.onedrive.shareUrl}
              />
            </SettingRow>

            <SettingRow
              title="Microsoft account"
              description={
                microsoft.signedIn
                  ? `Signed in${microsoft.account ? ` as ${microsoft.account}` : ''}${microsoft.expiresAt ? ` · token expires ${formatSyncTime(microsoft.expiresAt)}` : ''}`
                  : microsoft.customClientId
                    ? 'Uses your custom Azure AD app (AX_MS_CLIENT_ID) via device code sign-in.'
                    : 'Uses the built-in Microsoft app via device code sign-in — no Azure setup needed. Override with AX_MS_CLIENT_ID if your tenant restricts first-party app consent.'
              }
            >
              <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                {microsoft.signedIn ? (
                  <button
                    type="button"
                    className="btn btn-subtle"
                    disabled={!!busy}
                    onClick={() => void signOutMs()}
                  >
                    {busy === 'ms-out' ? 'Signing out…' : 'Sign out'}
                  </button>
                ) : (
                  <button
                    type="button"
                    className="btn primary"
                    disabled={!loaded || !!busy}
                    onClick={() => void startMicrosoftSignIn()}
                  >
                    {busy === 'ms-start' ? 'Connecting…' : 'Connect OneDrive'}
                  </button>
                )}
              </div>
            </SettingRow>
          </>
        )}

        {config.provider === 'github' && (
          <>
            <SettingRow title="Repository URL" description="HTTPS git URL (private repos use GITHUB_TOKEN env).">
              <input
                className="settings-input settings-input--wide settings-input--mono"
                value={config.github.repoUrl}
                disabled={!loaded || !!busy}
                onChange={(e) => patch({ github: { ...config.github, repoUrl: e.target.value } })}
                onBlur={() => void saveCurrent().catch(() => undefined)}
                placeholder="https://github.com/org/repo.git"
              />
            </SettingRow>
            <SettingRow title="Branch" description="Branch to pull from.">
              <input
                className="settings-input"
                value={config.github.branch}
                disabled={!loaded || !!busy}
                onChange={(e) => patch({ github: { ...config.github, branch: e.target.value } })}
                onBlur={() => void saveCurrent().catch(() => undefined)}
              />
            </SettingRow>
            <SettingRow title="Subpath" description="Folder inside the repo (default .ax).">
              <input
                className="settings-input settings-input--mono"
                value={config.github.subpath}
                disabled={!loaded || !!busy}
                onChange={(e) => patch({ github: { ...config.github, subpath: e.target.value } })}
                onBlur={() => void saveCurrent().catch(() => undefined)}
              />
            </SettingRow>

            {isHostedToolsGitlab(config.github.repoUrl) ? (
              <div className="settings-toast settings-toast--warn">
                <strong>gitlab.hosted-tools.com detected.</strong> This instance requires SSH keys to be
                uploaded via{' '}
                <a href={HOSTED_TOOLS_BRIDGE_URL} target="_blank" rel="noopener noreferrer">
                  Bridge
                </a>{' '}
                ({HOSTED_TOOLS_BRIDGE_URL}), which syncs them to GitLab — not GitLab&apos;s own SSH Keys
                settings page directly. Its HTTP/REST API is gated behind interactive SSO and is not open
                for automation/AI integrations, so use a plain <code>git@gitlab.hosted-tools.com:…</code>{' '}
                SSH URL above and leave the API token below blank.
              </div>
            ) : (
              <SettingRow
                title="API token (optional)"
                description="Only needed on locked-down GitLab instances where raw git (SSH/HTTPS) is forced through SSO and can't be used headlessly. When set, sync uses that host's /api/v4 REST API instead of git clone/push. Leave blank to use normal git credentials."
              >
                <input
                  type="password"
                  className="settings-input settings-input--wide settings-input--mono"
                  value={config.github.token}
                  disabled={!loaded || !!busy}
                  autoComplete="off"
                  onChange={(e) => patch({ github: { ...config.github, token: e.target.value } })}
                  onBlur={() => void saveCurrent().catch(() => undefined)}
                  placeholder="glpat-… / personal or project access token"
                />
              </SettingRow>
            )}
          </>
        )}

        <div className="settings-divider" />
        <div className="settings-subsection-label">Content &amp; import</div>

        <SettingRow title="Rules" description="Sync shared rules from remote pack.">
          <Toggle
            checked={config.content.rules}
            disabled={!loaded || !!busy}
            label="Sync rules"
            onChange={(v) => void persist(patch({ content: { ...config.content, rules: v } }))}
          />
        </SettingRow>
        <SettingRow title="Skills" description="Sync shared skills from remote pack.">
          <Toggle
            checked={config.content.skills}
            disabled={!loaded || !!busy}
            label="Sync skills"
            onChange={(v) => void persist(patch({ content: { ...config.content, skills: v } }))}
          />
        </SettingRow>
        <SettingRow title="Memory" description="Import memory/shared.jsonl (upsert by id).">
          <Toggle
            checked={config.content.memory}
            disabled={!loaded || !!busy}
            label="Sync memory"
            onChange={(v) => void persist(patch({ content: { ...config.content, memory: v } }))}
          />
        </SettingRow>

        <SettingRow title="Import mode" description={importModeHelp}>
          <select
            className="settings-select"
            value={config.importMode}
            disabled={!loaded || !!busy}
            aria-label="Import mode"
            onChange={(e) =>
              void persist(
                patch({ importMode: e.target.value as ShareConfig['importMode'] }),
                `Import mode: ${IMPORT_MODE_OPTIONS.find((o) => o.value === e.target.value)?.label ?? e.target.value}`,
              )
            }
          >
            {IMPORT_MODE_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </SettingRow>

        <SettingRow title="Auto-sync interval" description="Background pull interval when ax web is running (0 = manual only).">
          <select
            className="settings-select"
            value={String(config.autoSyncMinutes)}
            disabled={!loaded || !!busy}
            aria-label="Auto-sync interval"
            onChange={(e) =>
              void persist(
                patch({ autoSyncMinutes: Number(e.target.value) }),
                e.target.value === '0' ? 'Auto-sync disabled' : `Auto-sync every ${e.target.value} minutes`,
              )
            }
          >
            {AUTO_SYNC_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
            {!AUTO_SYNC_OPTIONS.some((o) => o.value === config.autoSyncMinutes) && (
              <option value={config.autoSyncMinutes}>Every {config.autoSyncMinutes} minutes</option>
            )}
          </select>
        </SettingRow>

        <div className="settings-divider" />
        <div className="settings-subsection-label">Sync status</div>

        <SyncStatusPanel sync={sync} microsoft={microsoft} provider={config.provider} />

        <div className="settings-row">
          <div className="settings-row-label">
            <span className="settings-row-title">Sync now</span>
            <span className="settings-row-desc">Pull remote changes and import into this project.</span>
          </div>
          <div className="settings-row-control" style={{ gap: 8 }}>
            <button type="button" className="btn primary" disabled={!loaded || !!busy} onClick={() => void runSync()}>
              {busy === 'sync' ? <BusyLabel label="Syncing…" /> : 'Sync now'}
            </button>
            <button type="button" className="btn btn-subtle" disabled={!loaded || !!busy} onClick={() => void refreshStatus()}>
              Refresh status
            </button>
          </div>
        </div>
      </div>

      <div className="settings-card-footer">
        <button type="button" className="btn primary" disabled={!loaded || !!busy} onClick={() => void saveCurrent()}>
          {busy === 'save' ? <BusyLabel label="Saving…" /> : 'Save settings'}
        </button>
      </div>

      {msModal && (
        <MicrosoftSignInModal
          flow={msModal}
          busy={msPolling}
          error={msModalErr}
          onClose={closeMsModal}
          onOpenUri={() =>
            window.open(
              msModal.verificationUriComplete || msModal.verificationUri,
              '_blank',
              'noopener,noreferrer',
            )
          }
        />
      )}
    </section>
  );
}

function emptySyncStatus(): ShareSyncStatus {
  return {
    rulesAdded: 0,
    skillsAdded: 0,
    rulesPending: 0,
    skillsPending: 0,
    memoryInserted: 0,
    memoryUpdated: 0,
    remoteFiles: 0,
  };
}

function SyncStatusPanel({
  sync,
  microsoft,
  provider,
}: {
  sync: ShareSyncStatus;
  microsoft: MicrosoftAuthStatus;
  provider: ShareProvider;
}) {
  return (
    <div className="settings-status-panel">
      <div className="settings-status-grid settings-status-grid--metrics">
        <div className="settings-status-pill">
          <div className="settings-status-pill-body">
            <span className="settings-status-pill-label">Last sync</span>
            <span className="settings-status-pill-value">{formatSyncTime(sync.lastSyncAt)}</span>
          </div>
        </div>
        <div className="settings-status-pill">
          <div className="settings-status-pill-body">
            <span className="settings-status-pill-label">Provider</span>
            <span className="settings-status-pill-value">{sync.provider ?? provider}</span>
          </div>
        </div>
        <div className="settings-status-pill">
          <div className="settings-status-pill-body">
            <span className="settings-status-pill-label">Imported</span>
            <span className="settings-status-pill-value">
              +{sync.rulesAdded} rules · +{sync.skillsAdded} skills
            </span>
          </div>
        </div>
        <div className="settings-status-pill">
          <div className="settings-status-pill-body">
            <span className="settings-status-pill-label">Pending review</span>
            <span className="settings-status-pill-value">
              {sync.rulesPending} rules · {sync.skillsPending} skills
            </span>
          </div>
        </div>
        {sync.memoryInserted > 0 || sync.memoryUpdated > 0 ? (
          <div className="settings-status-pill">
            <div className="settings-status-pill-body">
              <span className="settings-status-pill-label">Memory</span>
              <span className="settings-status-pill-value">
                +{sync.memoryInserted} new · ~{sync.memoryUpdated} updated
              </span>
            </div>
          </div>
        ) : null}
        {provider === 'onedrive' && (
          <div className="settings-status-pill">
            <div className="settings-status-pill-body">
              <span className="settings-status-pill-label">Microsoft</span>
              <span className="settings-status-pill-value">
                {microsoft.signedIn ? microsoft.account ?? 'Signed in' : 'Not signed in'}
              </span>
            </div>
          </div>
        )}
      </div>
      {sync.lastError && (
        <p className="settings-inline-err" role="alert" style={{ marginTop: 8 }}>
          Last error: {sync.lastError}
        </p>
      )}
    </div>
  );
}
