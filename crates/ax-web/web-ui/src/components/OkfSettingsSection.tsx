import { useCallback, useEffect, useState } from 'react';

import {
  exportOkf,
  fetchOkfConfig,
  publishOkf,
  validateOkf,
  type OkfConfigInfo,
  type OkfExportReport,
  type OkfValidateReport,
} from '../api';
import ModalShell from './ModalShell';

const DANGLING_PREVIEW = 8;

/** Shorten long OKF link diagnostics for the Settings card. */
function shortenLink(link: string, max = 72): string {
  const normalized = link.replace(/\\/g, '/').replace(/\s+/g, ' ').trim();
  if (normalized.length <= max) return normalized;
  const arrow = normalized.includes(' → ')
    ? ' → '
    : normalized.includes(' -> ')
      ? ' -> '
      : null;
  if (!arrow) {
    return `${normalized.slice(0, max - 1)}…`;
  }
  const [from, to] = normalized.split(arrow);
  const budget = Math.floor((max - arrow.length) / 2);
  const clip = (s: string) =>
    s.length <= budget ? s : `…${s.slice(-(budget - 1))}`;
  return `${clip(from)}${arrow}${clip(to)}`;
}

/** Settings → Open Knowledge Format (OKF): generate / validate / optional wiki publish. */
export default function OkfSettingsSection() {
  const [cfg, setCfg] = useState<OkfConfigInfo | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [lastExport, setLastExport] = useState<OkfExportReport | null>(null);
  const [lastValidate, setLastValidate] = useState<OkfValidateReport | null>(null);
  const [publishOpen, setPublishOpen] = useState(false);
  const [dryRun, setDryRun] = useState(true);
  const [tick, setTick] = useState(0);

  const reload = useCallback(() => {
    setTick((t) => t + 1);
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    fetchOkfConfig()
      .then((d) => {
        if (cancelled) return;
        setCfg(d);
        setErr(null);
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setErr(e instanceof Error ? e.message : 'Failed to load OKF config');
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [tick]);

  async function runExport() {
    setBusy('export');
    setErr(null);
    setMsg(null);
    setLastValidate(null);
    try {
      const report = await exportOkf();
      setLastExport(report);
      setMsg(
        `Exported ${report.exported} concepts into ${report.outDir}`,
      );
      reload();
    } catch (e: unknown) {
      setErr(e instanceof Error ? e.message : 'OKF export failed');
    } finally {
      setBusy(null);
    }
  }

  async function runValidate() {
    setBusy('validate');
    setErr(null);
    setMsg(null);
    try {
      const report = await validateOkf();
      setLastValidate(report);
      if (report.ok) {
        setMsg(`OKF — no issues found (${report.pages} concept pages)`);
      }
    } catch (e: unknown) {
      setLastValidate(null);
      setErr(e instanceof Error ? e.message : 'OKF validate failed');
    } finally {
      setBusy(null);
    }
  }

  async function runPublish() {
    setBusy('publish');
    setErr(null);
    setMsg(null);
    try {
      const report = await publishOkf({ dryRun, noPush: false });
      if (report.dryRun) {
        setMsg(
          `OKF wiki dry-run: ${report.filesCopied} file(s) → ${report.subdir}`,
        );
      } else {
        setMsg(
          `OKF wiki: ${report.wikiAction} — copied ${report.filesCopied} file(s)` +
            (report.committed
              ? report.pushed
                ? ', committed and pushed'
                : ', committed'
              : ', no changes'),
        );
      }
      setPublishOpen(false);
      reload();
    } catch (e: unknown) {
      setErr(e instanceof Error ? e.message : 'OKF wiki publish failed');
    } finally {
      setBusy(null);
    }
  }

  const wikiReady = !!(cfg?.wikiEnabled && cfg.wikiRemoteConfigured);
  const statusLabel = loading
    ? 'Loading…'
    : cfg?.enabled === false
      ? 'Export disabled'
      : cfg?.bundleExists
        ? 'Bundle present'
        : 'No bundle yet';

  return (
    <>
      <div className="settings-row">
        <div className="settings-row-label">
          <span className="settings-row-title">Bundle actions</span>
          <span className="settings-row-desc">
            Same engine as <code>ax export okf</code>. Output path comes from{' '}
            <code>okf.outDir</code> in <code>ax.json</code>.
            {!loading && cfg ? ` Status: ${statusLabel}.` : ''}
          </span>
        </div>
        <div className="settings-row-control settings-row-control--actions">
          <button type="button" className="btn btn-subtle" disabled={loading} onClick={reload}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={!!busy || loading || cfg?.enabled === false}
            onClick={() => void runExport()}
          >
            {busy === 'export' ? 'Generating…' : 'Generate OKF bundle'}
          </button>
          <button
            type="button"
            className="btn btn-subtle"
            disabled={!!busy || loading}
            onClick={() => void runValidate()}
          >
            {busy === 'validate' ? 'Validating…' : 'Validate'}
          </button>
          {cfg?.wikiEnabled && (
            <button
              type="button"
              className="btn btn-subtle"
              disabled={!!busy || loading || !wikiReady}
              title={
                wikiReady
                  ? 'Publish OKF bundle to configured wiki git remote'
                  : 'Set okf.azdoWiki.remote in ax.json'
              }
              onClick={() => {
                setDryRun(true);
                setPublishOpen(true);
              }}
            >
              Publish to wiki
            </button>
          )}
        </div>
      </div>

      {cfg && (
        <dl className="settings-meta">
          <div className="settings-meta-row">
            <dt>Relative</dt>
            <dd>
              <code>{cfg.outDir}</code>
            </dd>
          </div>
          <div className="settings-meta-row">
            <dt>Absolute</dt>
            <dd>
              <code>{cfg.outDirAbs}</code>
            </dd>
          </div>
          {cfg.wikiEnabled && (
            <div className="settings-meta-row">
              <dt>Wiki</dt>
              <dd>
                {cfg.wikiRemoteConfigured ? 'Remote configured' : 'Remote missing'} →{' '}
                <code>{cfg.wikiSubdir}</code>
              </dd>
            </div>
          )}
          {!cfg.enabled && (
            <div className="settings-meta-row">
              <dt>State</dt>
              <dd>
                Export disabled (<code>okf.enabled=false</code>)
              </dd>
            </div>
          )}
        </dl>
      )}

      {lastExport && (
        <p className="settings-inline-ok" role="status">
          Last export: {lastExport.exported} concepts
          {Object.keys(lastExport.byKind).length > 0 &&
            ` (${Object.entries(lastExport.byKind)
              .map(([k, n]) => `${k}: ${n}`)
              .join(', ')})`}
        </p>
      )}

      {msg && (
        <p className="settings-inline-ok" role="status">
          {msg}
        </p>
      )}

      {lastValidate && !lastValidate.ok && (
        <div className="settings-report settings-report--warn" role="alert">
          <div className="settings-report-title">
            Validation failed
            <span className="settings-report-count">
              {(lastValidate.missingIndex ? 1 : 0) + lastValidate.danglingLinks.length}{' '}
              issue
              {(lastValidate.missingIndex ? 1 : 0) + lastValidate.danglingLinks.length === 1
                ? ''
                : 's'}
              {' · '}
              {lastValidate.pages} pages
            </span>
          </div>
          {lastValidate.missingIndex && (
            <p className="settings-report-note">Missing <code>index.md</code> in the OKF out dir.</p>
          )}
          {lastValidate.danglingLinks.length > 0 && (
            <>
              <p className="settings-report-note">
                Dangling links (showing{' '}
                {Math.min(DANGLING_PREVIEW, lastValidate.danglingLinks.length)} of{' '}
                {lastValidate.danglingLinks.length}):
              </p>
              <ul className="settings-report-list">
                {lastValidate.danglingLinks.slice(0, DANGLING_PREVIEW).map((link) => (
                  <li key={link} title={link}>
                    <code>{shortenLink(link)}</code>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}

      {err && (
        <p className="settings-inline-err" role="alert">
          {err}
        </p>
      )}

      {publishOpen && (
        <ModalShell
          title="Publish OKF to wiki"
          subtitle="Copies the Open Knowledge Format (OKF) bundle into the configured wiki git remote."
          onClose={() => !busy && setPublishOpen(false)}
          footer={
            <>
              <button
                type="button"
                className="btn btn-subtle"
                disabled={!!busy}
                onClick={() => setPublishOpen(false)}
              >
                Cancel
              </button>
              <button
                type="button"
                className="btn primary"
                disabled={!!busy}
                onClick={() => void runPublish()}
              >
                {busy === 'publish'
                  ? 'Publishing…'
                  : dryRun
                    ? 'Dry-run publish'
                    : 'Publish & push'}
              </button>
            </>
          }
        >
          <div className="ax-modal-form-stack">
            <label className="ax-modal-form-row" style={{ gap: 8, alignItems: 'center' }}>
              <input
                type="checkbox"
                checked={dryRun}
                disabled={!!busy}
                onChange={(e) => setDryRun(e.target.checked)}
              />
              <span>
                Dry-run only (preview destination; no clone / commit / push)
              </span>
            </label>
            {!dryRun && (
              <p className="settings-row-desc">
                Live publish uses your local git credentials for the wiki remote. No tokens are
                stored in ax.
              </p>
            )}
          </div>
        </ModalShell>
      )}
    </>
  );
}
