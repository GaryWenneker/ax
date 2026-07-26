import { useCallback, useEffect, useState } from 'react';
import ModalShell from './ModalShell';

const PWA_DISMISS_KEY = 'ax-pwa-install-dismissed';
const PWA_OPTIN_KEY = 'ax-pwa-optin';

function isStandalone(): boolean {
  if (typeof window === 'undefined') return false;
  const nav = window.navigator as Navigator & { standalone?: boolean };
  return window.matchMedia('(display-mode: standalone)').matches || nav.standalone === true;
}

function isIos(): boolean {
  if (typeof navigator === 'undefined') return false;
  return /iphone|ipad|ipod/i.test(navigator.userAgent);
}

/** Settings → Sharing: status card + how-to + PWA opt-in/install. */
export default function SharingSettingsSection() {
  const [sharing, setSharing] = useState(false);
  const [readonly, setReadonly] = useState(false);
  const [port, setPort] = useState(7070);
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState<string | null>(null);
  const [helpOpen, setHelpOpen] = useState(false);
  const [copied, setCopied] = useState<'cli' | 'url' | null>(null);
  const [pwaDismissed, setPwaDismissed] = useState(
    () => typeof localStorage !== 'undefined' && localStorage.getItem(PWA_DISMISS_KEY) === '1',
  );
  const [pwaOptIn, setPwaOptIn] = useState(
    () =>
      typeof localStorage !== 'undefined' &&
      (localStorage.getItem(PWA_OPTIN_KEY) === '1' ||
        new URLSearchParams(window.location.search).has('pwa') ||
        isStandalone()),
  );
  const [deferredPrompt, setDeferredPrompt] = useState<{ prompt: () => Promise<void> } | null>(
    null,
  );

  const loadStatus = useCallback(() => {
    setLoading(true);
    setErr(null);
    fetch('/api/share/status')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((d: { sharing?: boolean; readonly?: boolean; port?: number }) => {
        setSharing(!!d.sharing);
        setReadonly(!!d.readonly);
        if (typeof d.port === 'number') setPort(d.port);
        setErr(null);
      })
      .catch((e: unknown) => {
        setErr(e instanceof Error ? e.message : 'Failed to load share status');
      })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    loadStatus();

    function onBeforeInstall(e: Event) {
      e.preventDefault();
      const ev = e as Event & { prompt: () => Promise<void> };
      setDeferredPrompt({ prompt: () => ev.prompt() });
    }
    window.addEventListener('beforeinstallprompt', onBeforeInstall);
    return () => window.removeEventListener('beforeinstallprompt', onBeforeInstall);
  }, [loadStatus]);

  const baseUrl = window.location.origin;
  const shareUrlExample = `${baseUrl}/?token=…`;

  async function copyHint() {
    try {
      await navigator.clipboard.writeText(
        `ax share --open --port ${port}\n# then open ${baseUrl}/?token=<token from CLI>`,
      );
      setCopied('cli');
      window.setTimeout(() => setCopied(null), 2000);
    } catch {
      /* ignore */
    }
  }

  async function copyBaseUrl() {
    try {
      await navigator.clipboard.writeText(baseUrl);
      setCopied('url');
      window.setTimeout(() => setCopied(null), 2000);
    } catch {
      /* ignore */
    }
  }

  function dismissPwa() {
    localStorage.setItem(PWA_DISMISS_KEY, '1');
    setPwaDismissed(true);
  }

  function showPwaAgain() {
    localStorage.removeItem(PWA_DISMISS_KEY);
    setPwaDismissed(false);
  }

  function enablePwa() {
    localStorage.setItem(PWA_OPTIN_KEY, '1');
    setPwaOptIn(true);
    const url = new URL(window.location.href);
    url.searchParams.set('pwa', '1');
    window.location.assign(url.toString());
  }

  function disablePwa() {
    localStorage.removeItem(PWA_OPTIN_KEY);
    setPwaOptIn(false);
    const url = new URL(window.location.href);
    url.searchParams.delete('pwa');
    // Reload so main.tsx unregisters the service worker.
    window.location.assign(url.pathname + url.search + url.hash);
  }

  async function installPwa() {
    if (deferredPrompt) {
      await deferredPrompt.prompt();
      setDeferredPrompt(null);
    }
    dismissPwa();
  }

  const statusLabel = loading
    ? 'Checking…'
    : sharing
      ? 'Shared (read-only)'
      : readonly
        ? 'Read-only'
        : 'Local';

  const statusTone = sharing || readonly ? 'shared' : 'local';

  return (
    <>
      <div className="setting-row">
        <div>
          <div className="setting-row-title">Share session</div>
          <div className="setting-row-desc">
            {err
              ? err
              : sharing
                ? 'This window is a shared read-only session (AX_SHARE_TOKEN).'
                : readonly
                  ? 'Read-only mode is active.'
                  : 'Local session. Use `ax share` to expose Command Center on the LAN with a token.'}
          </div>
          {!err && (
            <div className="settings-share-meta">
              <span className={`settings-share-badge settings-share-badge--${statusTone}`}>
                {statusLabel}
              </span>
              <span className="mono">port {port}</span>
              <button type="button" className="status-panel-link" onClick={() => void copyBaseUrl()}>
                {copied === 'url' ? 'URL copied' : 'Copy base URL'}
              </button>
            </div>
          )}
        </div>
        <div className="setting-row-control" style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          <button type="button" className="btn" disabled={loading} onClick={() => loadStatus()}>
            Refresh
          </button>
          <button type="button" className="btn" onClick={() => setHelpOpen(true)}>
            How to share
          </button>
          <button type="button" className="btn" onClick={() => void copyHint()}>
            {copied === 'cli' ? 'Copied' : 'Copy CLI tip'}
          </button>
        </div>
      </div>

      {err && (
        <p className="settings-inline-err" role="alert">
          {err}{' '}
          <button type="button" className="status-panel-link" onClick={() => loadStatus()}>
            Retry
          </button>
        </p>
      )}

      <div className="setting-row">
        <div>
          <div className="setting-row-title">Install as app (PWA)</div>
          <div className="setting-row-desc">
            {pwaOptIn
              ? deferredPrompt
                ? 'PWA enabled — Install is available in this browser.'
                : isIos()
                  ? 'PWA enabled. On iOS: Share → Add to Home Screen.'
                  : isStandalone()
                    ? 'Running as an installed app.'
                    : 'PWA enabled. Use the browser Install / Add to Home Screen menu if Install does not appear.'
              : 'Opt in to register the service worker, then Install (or Add to Home Screen). Off by default to avoid stale cache issues.'}
          </div>
        </div>
        <div className="setting-row-control" style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          {!pwaOptIn ? (
            <button type="button" className="btn primary" onClick={enablePwa}>
              Enable PWA
            </button>
          ) : (
            <>
              {deferredPrompt && (
                <button type="button" className="btn primary" onClick={() => void installPwa()}>
                  Install
                </button>
              )}
              {!isStandalone() && (
                <button type="button" className="btn" onClick={disablePwa}>
                  Disable PWA
                </button>
              )}
            </>
          )}
          {pwaDismissed ? (
            <button type="button" className="btn" onClick={showPwaAgain}>
              Show hint again
            </button>
          ) : (
            pwaOptIn && (
              <button type="button" className="btn" onClick={dismissPwa}>
                Dismiss hint
              </button>
            )
          )}
        </div>
      </div>

      {helpOpen && (
        <ModalShell
          title="Share Command Center"
          subtitle="Temporary LAN access with a bearer token (read-only)."
          onClose={() => setHelpOpen(false)}
          footer={
            <button type="button" className="btn" onClick={() => setHelpOpen(false)}>
              Close
            </button>
          }
        >
          <ol className="settings-help-list">
            <li>
              Run <code>ax share --open --port {port}</code> in the project root.
            </li>
            <li>
              Open the printed URL (includes <code>?token=</code>) or paste the token on the gate page.
            </li>
            <li>
              For remote collaborators:{' '}
              <code>cloudflared tunnel --url http://127.0.0.1:{port}</code>
            </li>
          </ol>
          <p className="status-panel-muted">
            Base URL: <code>{baseUrl}</code> · example shape: {shareUrlExample}
          </p>
        </ModalShell>
      )}
    </>
  );
}
