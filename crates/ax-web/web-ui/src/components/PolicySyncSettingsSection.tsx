import { useEffect, useState } from 'react';
import {
  exportPolicyPack,
  fetchPolicySyncSettings,
  importPolicyPack,
  savePolicySyncSettings,
} from '../policyApi';
import type { PolicySyncSettings } from '../policyTypes';

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

export default function PolicySyncSettingsSection() {
  const [settings, setSettings] = useState<PolicySyncSettings | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    fetchPolicySyncSettings()
      .then(setSettings)
      .catch((e: Error) => setErr(e.message));
  }, []);

  async function patch(next: Partial<PolicySyncSettings>) {
    setBusy('save');
    setErr(null);
    setMsg(null);
    try {
      const saved = await savePolicySyncSettings(next);
      setSettings(saved);
      setMsg('Policy sync settings saved to ax.json');
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  async function runExport() {
    setBusy('export');
    setErr(null);
    setMsg(null);
    try {
      const r = await exportPolicyPack();
      setMsg(`Exported ${r.rulesExported} rules, ${r.skillsExported} skills → ${r.path}`);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  async function runImport() {
    setBusy('import');
    setErr(null);
    setMsg(null);
    try {
      const r = await importPolicyPack(false);
      setMsg(`Imported pack (pending ${r.rulesPending ?? 0}/${r.skillsPending ?? 0})`);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <section className="settings-card">
      <div className="settings-card-header">
        <h2>Policy sync</h2>
        <p>
          Per-project pack for project/workspace rules and skills. Full UI:{' '}
          <a href="/policy/sync">Policy → Sync</a>. Separate from LAN Command Center sharing.
        </p>
      </div>
      <div className="settings-card-body">
        {msg && <div className="settings-toast settings-toast--ok">{msg}</div>}
        {err && <div className="settings-toast settings-toast--err">{err}</div>}

        <div className="settings-row">
          <div className="settings-row-label">
            <span className="settings-row-title">policySync hooks</span>
            <span className="settings-row-desc">
              Post-commit export and post-merge import of <code>.ax/policy/shared/</code> (run{' '}
              <code>ax sync</code> once after enabling).
            </span>
          </div>
          <div className="settings-row-control">
            <Toggle
              checked={!!settings?.policySync}
              disabled={!settings || !!busy}
              label="policySync"
              onChange={(v) => patch({ policySync: v })}
            />
          </div>
        </div>

        <div className="settings-row">
          <div className="settings-row-label">
            <span className="settings-row-title">Require review</span>
            <span className="settings-row-desc">
              Pack imports land in <code>.ax/policy/pending/</code> until approved (PR-style).
            </span>
          </div>
          <div className="settings-row-control">
            <Toggle
              checked={!!settings?.requireReview}
              disabled={!settings || !!busy}
              label="requireReview"
              onChange={(v) => patch({ requireReview: v })}
            />
          </div>
        </div>

        <div className="settings-row">
          <div className="settings-row-label">
            <span className="settings-row-title">Pack actions</span>
            <span className="settings-row-desc">Manual export / import for this project.</span>
          </div>
          <div className="settings-row-control" style={{ gap: 8 }}>
            <button type="button" className="btn btn-subtle" disabled={!!busy} onClick={runExport}>
              {busy === 'export' ? 'Exporting…' : 'Export pack'}
            </button>
            <button type="button" className="btn btn-subtle" disabled={!!busy} onClick={runImport}>
              {busy === 'import' ? 'Importing…' : 'Import pack'}
            </button>
          </div>
        </div>
      </div>
    </section>
  );
}
