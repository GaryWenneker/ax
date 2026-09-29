import { useCallback, useEffect, useState } from 'react';
import {
  connectVault,
  disconnectVault,
  fetchMountStatus,
  mountError,
  mountSummary,
  openLabel,
  openVault,
  validMountName,
  type MountError,
  type MountStatus,
} from '../vaultMount';

function asMountError(e: unknown): MountError {
  if (e && typeof e === 'object' && 'message' in e && 'command' in e) return e as MountError;
  return mountError(0, { error: e instanceof Error ? e.message : String(e) });
}

/** Settings → Vault connection: mount the WebDAV vault as a drive named "ax". */
export default function VaultMountCard() {
  const [status, setStatus] = useState<MountStatus | null>(null);
  const [name, setName] = useState('ax');
  const [autostart, setAutostart] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<MountError | null>(null);
  const [copied, setCopied] = useState(false);

  const apply = useCallback((s: MountStatus) => {
    setStatus(s);
    setName(s.name);
    setAutostart(s.autostart);
    setErr(null);
  }, []);

  useEffect(() => {
    fetchMountStatus().then(apply, (e: unknown) => setErr(asMountError(e)));
  }, [apply]);

  async function run(action: () => Promise<MountStatus>) {
    setBusy(true);
    try {
      apply(await action());
    } catch (e) {
      setErr(asMountError(e));
    } finally {
      setBusy(false);
    }
  }

  async function copyCommand(command: string) {
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      /* ignore */
    }
  }

  const nameOk = validMountName(name);

  return (
    <>
      <div className="settings-row">
        <div className="settings-row-label">
          <span className="settings-row-title">Vault drive</span>
          <span className="settings-row-desc">
            {status ? mountSummary(status) : 'Checking…'}
            {status && (
              <>
                {' '}
                · <span className="mono">{status.url}</span>
              </>
            )}
          </span>
        </div>
        <div className="settings-row-control settings-row-control--actions">
          {status?.mounted ? (
            <>
              <button
                type="button"
                className="btn primary"
                disabled={busy}
                onClick={() => void run(openVault)}
              >
                {openLabel(status.os)}
              </button>
              <button
                type="button"
                className="btn btn-subtle"
                disabled={busy}
                onClick={() => void run(disconnectVault)}
              >
                Disconnect
              </button>
            </>
          ) : (
            <button
              type="button"
              className="btn primary"
              disabled={busy || !nameOk || !status}
              onClick={() => void run(() => connectVault(name, autostart))}
            >
              {busy ? 'Connecting…' : `Connect as "${name}"`}
            </button>
          )}
        </div>
      </div>

      <div className="settings-row">
        <div className="settings-row-label">
          <span className="settings-row-title">Drive name</span>
          <span className="settings-row-desc">
            {nameOk
              ? 'Shown in Finder, Explorer, or Files. Windows picks the first free drive letter.'
              : 'Use 1–32 letters, digits, spaces, "_" or "-".'}
          </span>
        </div>
        <div className="settings-row-control">
          <input
            className="settings-input settings-input--narrow"
            value={name}
            disabled={busy || status?.mounted}
            aria-invalid={!nameOk}
            aria-label="Drive name"
            onChange={(e) => setName(e.target.value)}
          />
        </div>
      </div>

      <div className="settings-row">
        <div className="settings-row-label">
          <span className="settings-row-title">Connect at login</span>
          <span className="settings-row-desc">
            Reconnects when you log in. Needs `ax web` to be running.
          </span>
        </div>
        <div className="settings-row-control">
          <input
            type="checkbox"
            checked={autostart}
            disabled={busy || status?.mounted}
            aria-label="Connect at login"
            onChange={(e) => setAutostart(e.target.checked)}
          />
        </div>
      </div>

      {err && (
        <p className="settings-inline-err" role="alert">
          {err.message}
          {err.command && (
            <>
              {' '}
              <code>{err.command}</code>{' '}
              <button
                type="button"
                className="status-panel-link"
                onClick={() => void copyCommand(err.command ?? '')}
              >
                {copied ? 'Copied' : 'Copy command'}
              </button>
            </>
          )}
        </p>
      )}
    </>
  );
}
