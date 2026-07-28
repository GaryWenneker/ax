import { useCallback, useEffect, useState } from 'react';
import {
  exportPolicyPack,
  fetchPolicyPackStatus,
  fetchPolicyReview,
  fetchPolicySyncSettings,
  importPolicyPack,
  savePolicySyncSettings,
} from '../policyApi';
import type { PolicyPackStatus, PolicySyncSettings } from '../policyTypes';
import {
  PageCard,
  PageCardBody,
  PageHero,
  PageLoading,
  PageShell,
  PageStack,
  PageToasts,
} from '../components/ui/PageLayout';
import { usePageContext } from '../context/UiContext';

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

interface Props {
  onOpenReview: () => void;
}

export default function PolicySyncPage({ onOpenReview }: Props) {
  const [settings, setSettings] = useState<PolicySyncSettings | null>(null);
  const [status, setStatus] = useState<PolicyPackStatus | null>(null);
  const [pendingCount, setPendingCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState('');

  usePageContext('Policy sync', status ? `${status.localSharedRules} exportable rules` : undefined);

  const reload = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const [s, st, review] = await Promise.all([
        fetchPolicySyncSettings(),
        fetchPolicyPackStatus(),
        fetchPolicyReview().catch(() => ({ items: [] })),
      ]);
      setSettings(s);
      setStatus(st);
      setPendingCount(review.items.length);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load policy sync');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function patch(next: Partial<PolicySyncSettings>) {
    setBusy('save');
    setMsg(null);
    setError('');
    try {
      const saved = await savePolicySyncSettings(next);
      setSettings(saved);
      setMsg('Saved to ax.json — run ax sync once so git hooks pick up policySync.');
      const st = await fetchPolicyPackStatus();
      setStatus(st);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  async function runExport() {
    setBusy('export');
    setMsg(null);
    setError('');
    try {
      const r = await exportPolicyPack();
      setMsg(`Exported ${r.rulesExported} rules, ${r.skillsExported} skills → ${r.path}`);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  async function runImport(force: boolean) {
    setBusy(force ? 'force' : 'import');
    setMsg(null);
    setError('');
    try {
      const r = await importPolicyPack(force);
      setMsg(
        `Import done: +${r.rulesAdded ?? 0} rules, +${r.skillsAdded ?? 0} skills, pending ${r.rulesPending ?? 0}/${r.skillsPending ?? 0}, conflicts ${r.conflicts ?? 0}`,
      );
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return (
      <PageShell>
        <PageHero title="Policy sync" subtitle="Share rules and skills with teammates via git." />
        <PageLoading label="Loading pack status…" />
      </PageShell>
    );
  }

  return (
    <PageShell>
      <PageHero
        title="Policy sync"
        subtitle="Per-project pack under .ax/policy/shared/ — export project/workspace items, commit, import."
        actions={
          <>
            <button type="button" className="btn btn-subtle" disabled={!!busy} onClick={() => void reload()}>
              Refresh
            </button>
            <button type="button" className="btn primary" disabled={!!busy} onClick={() => void runExport()}>
              {busy === 'export' ? 'Exporting…' : 'Export pack'}
            </button>
            <button type="button" className="btn btn-subtle" disabled={!!busy} onClick={() => void runImport(false)}>
              {busy === 'import' ? 'Importing…' : 'Import pack'}
            </button>
          </>
        }
      />

      <PageToasts err={error || null} ok={msg} />

      <PageStack>
        <PageCard
          title="Status"
          description="Export includes project/workspace items (not company/private). Opt out with tags local or noshare."
        >
          <PageCardBody>
            <div className="settings-row">
              <div className="settings-row-label">
                <span className="settings-row-title">Pack path</span>
                <span className="settings-row-desc mono">{status?.packPath ?? '—'}</span>
              </div>
              <div className="settings-row-control">
                <span className="muted">{status?.hasManifest ? 'manifest present' : 'no manifest yet'}</span>
              </div>
            </div>
            <div className="settings-row">
              <div className="settings-row-label">
                <span className="settings-row-title">In pack</span>
                <span className="settings-row-desc">
                  {status?.rulesInPack ?? 0} rules · {status?.skillsInPack ?? 0} skills
                  {status?.tag ? ` · tag “${status.tag}”` : ''}
                </span>
              </div>
            </div>
            <div className="settings-row">
              <div className="settings-row-label">
                <span className="settings-row-title">Exportable locally</span>
                <span className="settings-row-desc">
                  {status?.localSharedRules ?? 0} rules · {status?.localSharedSkills ?? 0} skills
                  {' '}(project/workspace, enabled)
                </span>
              </div>
            </div>
            <div className="settings-row">
              <div className="settings-row-label">
                <span className="settings-row-title">Review queue</span>
                <span className="settings-row-desc">{pendingCount} pending item(s)</span>
              </div>
              <div className="settings-row-control">
                <button type="button" className="btn btn-subtle" onClick={onOpenReview}>
                  Open review
                </button>
              </div>
            </div>
          </PageCardBody>
        </PageCard>

        <PageCard title="Settings" description="Stored in project ax.json.">
          <PageCardBody>
            <div className="settings-row">
              <div className="settings-row-label">
                <span className="settings-row-title">policySync hooks</span>
                <span className="settings-row-desc">
                  Post-commit export + post-merge import of the pack (run <code>ax sync</code> after enabling).
                </span>
              </div>
              <div className="settings-row-control">
                <Toggle
                  checked={!!settings?.policySync}
                  disabled={!settings || !!busy}
                  label="policySync"
                  onChange={(v) => void patch({ policySync: v })}
                />
              </div>
            </div>
            <div className="settings-row">
              <div className="settings-row-label">
                <span className="settings-row-title">Require review</span>
                <span className="settings-row-desc">
                  Imports land in <code>.ax/policy/pending/</code> until approved.
                </span>
              </div>
              <div className="settings-row-control">
                <Toggle
                  checked={!!settings?.requireReview}
                  disabled={!settings || !!busy}
                  label="requireReview"
                  onChange={(v) => void patch({ requireReview: v })}
                />
              </div>
            </div>
            <div className="settings-row">
              <div className="settings-row-label">
                <span className="settings-row-title">Force import</span>
                <span className="settings-row-desc">Overwrite conflicting local items (skip pending staging).</span>
              </div>
              <div className="settings-row-control">
                <button
                  type="button"
                  className="btn btn-subtle"
                  disabled={!!busy}
                  onClick={() => {
                    if (confirm('Force-import pack and overwrite conflicting local items?')) {
                      void runImport(true);
                    }
                  }}
                >
                  {busy === 'force' ? 'Importing…' : 'Import --force'}
                </button>
              </div>
            </div>
          </PageCardBody>
        </PageCard>

        <PageCard title="CLI" description="Same operations from the terminal.">
          <PageCardBody>
            <pre className="page-code-block">{`ax policy pack export
ax policy pack import
ax policy pack status
ax policy review list
ax policy enable <id>
ax policy disable <id>`}</pre>
          </PageCardBody>
        </PageCard>
      </PageStack>
    </PageShell>
  );
}
