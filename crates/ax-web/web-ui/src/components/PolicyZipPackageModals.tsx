import { useEffect, useMemo, useState } from 'react';
import ModalShell from './ModalShell';
import {
  diffPolicyPackageItem,
  downloadPolicyPackage,
  fetchPolicyRules,
  fetchPolicySkills,
  previewPolicyPackage,
  restorePolicyPackage,
  type PolicyPackagePreviewItem,
} from '../policyApi';
import {
  compareStatusClass,
  compareSummary,
  emptyDiffCopy,
  policyItemDescription,
  restoreDecisionLabels,
  unifiedDiffLines,
} from '../policyPackage';
import { defaultRestoreAction, isShareablePolicyItem, type PolicyRuleRow, type PolicySkillRow } from '../policyTypes';

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export default function PolicyZipPackageButtons({ onRestored }: { onRestored?: () => void }) {
  const [composeOpen, setComposeOpen] = useState(false);
  const [restoreOpen, setRestoreOpen] = useState(false);
  return (
    <>
      <button type="button" className="btn btn-subtle" onClick={() => setComposeOpen(true)}>
        Package
      </button>
      <button type="button" className="btn btn-subtle" onClick={() => setRestoreOpen(true)}>
        Restore package
      </button>
      {composeOpen && <ComposeModal onClose={() => setComposeOpen(false)} />}
      {restoreOpen && (
        <RestoreModal
          onClose={() => setRestoreOpen(false)}
          onRestored={() => {
            setRestoreOpen(false);
            onRestored?.();
          }}
        />
      )}
    </>
  );
}

function UnifiedDiffView({ unified, compare }: { unified: string; compare: string }) {
  if (!unified) {
    return <p className="muted">{emptyDiffCopy(compare)}</p>;
  }
  return (
    <pre className="policy-pack-diff" aria-label="Unified diff">
      {unifiedDiffLines(unified).map((line, i) => (
        <span key={i} className={`policy-pack-diff-line policy-pack-diff-line--${line.kind}`}>
          {line.text || ' '}
          {'\n'}
        </span>
      ))}
    </pre>
  );
}

function ComposeModal({ onClose }: { onClose: () => void }) {
  const [name, setName] = useState('Team pack');
  const [description, setDescription] = useState('');
  const [rules, setRules] = useState<PolicyRuleRow[]>([]);
  const [skills, setSkills] = useState<PolicySkillRow[]>([]);
  const [ruleIds, setRuleIds] = useState<Set<string>>(new Set());
  const [skillNames, setSkillNames] = useState<Set<string>>(new Set());
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [inspect, setInspect] = useState<{ title: string; description: string; body: string } | null>(null);

  useEffect(() => {
    void Promise.all([fetchPolicyRules(), fetchPolicySkills()])
      .then(([r, s]) => {
        setRules(r.rules.filter((x) => isShareablePolicyItem(x.scope, x.enabled)));
        setSkills(s.skills.filter((x) => isShareablePolicyItem(x.scope, x.enabled)));
      })
      .catch((e) => setError(e instanceof Error ? e.message : 'Failed to load policy'));
  }, []);

  const canDownload = name.trim().length > 0 && (ruleIds.size > 0 || skillNames.size > 0);
  const ruleIdList = rules.map((r) => r.id);
  const skillNameList = skills.map((s) => s.name);

  async function download() {
    setBusy(true);
    setError('');
    try {
      const { blob, filename } = await downloadPolicyPackage({
        name: name.trim(),
        description: description.trim(),
        ruleIds: [...ruleIds],
        skillNames: [...skillNames],
      });
      downloadBlob(blob, filename);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Download failed');
    } finally {
      setBusy(false);
    }
  }

  function openRule(r: PolicyRuleRow) {
    const description = policyItemDescription({ id: r.id, body: r.body });
    setInspect({
      title: `Rule ${r.id}`,
      description,
      body: r.body,
    });
  }

  function openSkill(s: PolicySkillRow) {
    const description = policyItemDescription({ id: s.name, description: s.description, body: s.body });
    setInspect({
      title: `Skill ${s.name}`,
      description,
      body: s.body,
    });
  }

  return (
    <ModalShell
      size="xl"
      title="Package rules and skills"
      subtitle="Build a portable zip like a Sitecore package. Private and disabled items are omitted. Click a name to inspect local content."
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn btn-subtle" onClick={onClose}>
            Cancel
          </button>
          <button type="button" className="btn primary" disabled={!canDownload || busy} onClick={() => void download()}>
            {busy ? 'Building…' : 'Download'}
          </button>
        </>
      }
    >
      <div className="ax-modal-form-stack policy-pack-layout">
        {error && <p className="page-toast-err">{error}</p>}
        <label className="settings-field">
          <span>Name</span>
          <input className="settings-input" value={name} onChange={(e) => setName(e.target.value)} required />
        </label>
        <label className="settings-field">
          <span>Description</span>
          <input className="settings-input" value={description} onChange={(e) => setDescription(e.target.value)} />
        </label>
        <div className="policy-pack-split">
          <div className="policy-pack-columns">
            <fieldset className="policy-pack-col">
              <legend>Rules</legend>
              <div className="policy-pack-col-actions">
                <button
                  type="button"
                  className="btn btn-subtle"
                  disabled={rules.length === 0}
                  onClick={() => setRuleIds(new Set(ruleIdList))}
                >
                  Select all
                </button>
                <button type="button" className="btn btn-subtle" disabled={ruleIds.size === 0} onClick={() => setRuleIds(new Set())}>
                  Select none
                </button>
              </div>
              {rules.length === 0 && <p className="muted">No shareable rules</p>}
              {rules.map((r) => (
                <div key={r.id} className="policy-pack-check">
                  <input
                    type="checkbox"
                    checked={ruleIds.has(r.id)}
                    onChange={() => {
                      const next = new Set(ruleIds);
                      if (next.has(r.id)) next.delete(r.id);
                      else next.add(r.id);
                      setRuleIds(next);
                    }}
                  />
                  <button type="button" className="policy-pack-open" onClick={() => openRule(r)}>
                    <span className="policy-pack-item-title">{r.id}</span>
                    <span className="policy-pack-item-desc">{policyItemDescription({ id: r.id, body: r.body })}</span>
                  </button>
                </div>
              ))}
            </fieldset>
            <fieldset className="policy-pack-col">
              <legend>Skills</legend>
              <div className="policy-pack-col-actions">
                <button
                  type="button"
                  className="btn btn-subtle"
                  disabled={skills.length === 0}
                  onClick={() => setSkillNames(new Set(skillNameList))}
                >
                  Select all
                </button>
                <button
                  type="button"
                  className="btn btn-subtle"
                  disabled={skillNames.size === 0}
                  onClick={() => setSkillNames(new Set())}
                >
                  Select none
                </button>
              </div>
              {skills.length === 0 && <p className="muted">No shareable skills</p>}
              {skills.map((s) => (
                <div key={s.name} className="policy-pack-check">
                  <input
                    type="checkbox"
                    checked={skillNames.has(s.name)}
                    onChange={() => {
                      const next = new Set(skillNames);
                      if (next.has(s.name)) next.delete(s.name);
                      else next.add(s.name);
                      setSkillNames(next);
                    }}
                  />
                  <button type="button" className="policy-pack-open" onClick={() => openSkill(s)}>
                    <span className="policy-pack-item-title">{s.name}</span>
                    <span className="policy-pack-item-desc">
                      {policyItemDescription({ id: s.name, description: s.description, body: s.body })}
                    </span>
                  </button>
                </div>
              ))}
            </fieldset>
          </div>
          <aside className="policy-pack-inspect">
            {inspect ? (
              <>
                <h3 className="policy-pack-inspect-title">{inspect.title}</h3>
                {inspect.description ? <p className="policy-pack-item-desc">{inspect.description}</p> : null}
                <pre className="policy-pack-inspect-body">{inspect.body || '(empty)'}</pre>
              </>
            ) : (
              <p className="muted">Click a rule or skill name to inspect the local file.</p>
            )}
          </aside>
        </div>
      </div>
    </ModalShell>
  );
}

function RestoreModal({ onClose, onRestored }: { onClose: () => void; onRestored: () => void }) {
  const [file, setFile] = useState<File | null>(null);
  const [items, setItems] = useState<PolicyPackagePreviewItem[]>([]);
  const [packName, setPackName] = useState('');
  const [decisions, setDecisions] = useState<Record<string, 'overwrite' | 'skip'>>({});
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const [diffUnified, setDiffUnified] = useState('');
  const [diffCompare, setDiffCompare] = useState('');

  const canRestore = useMemo(
    () => file && items.some((i) => i.status !== 'invalid'),
    [file, items],
  );

  async function onFile(f: File | null) {
    setFile(f);
    setItems([]);
    setDecisions({});
    setError('');
    setActiveKey(null);
    setDiffUnified('');
    setDiffCompare('');
    if (!f) return;
    setBusy(true);
    try {
      const preview = await previewPolicyPackage(f);
      setPackName(preview.name);
      setItems(preview.items);
      const next: Record<string, 'overwrite' | 'skip'> = {};
      for (const item of preview.items) {
        const action = defaultRestoreAction(item.status, item.newer);
        if (action) next[`${item.kind}:${item.id}`] = action;
      }
      setDecisions(next);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Preview failed');
    } finally {
      setBusy(false);
    }
  }

  async function openDiff(item: PolicyPackagePreviewItem) {
    if (!file) return;
    const key = `${item.kind}:${item.id}`;
    setActiveKey(key);
    setError('');
    try {
      const diff = await diffPolicyPackageItem(file, item.kind, item.id);
      setDiffCompare(diff.compare);
      setDiffUnified(diff.unified);
    } catch (e) {
      setDiffUnified('');
      setDiffCompare(item.compare ?? item.status);
      setError(e instanceof Error ? e.message : 'Diff failed');
    }
  }

  async function confirm() {
    if (!file) return;
    setBusy(true);
    setError('');
    try {
      await restorePolicyPackage(file, decisions);
      onRestored();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Restore failed');
    } finally {
      setBusy(false);
    }
  }

  return (
    <ModalShell
      size="xl"
      title="Restore package"
      subtitle={
        packName
          ? `Preview: ${packName}. Click a row for a git-style diff.`
          : 'Upload an .ax-policy.zip, then Accept or Reject each item. Local newer files default to Reject.'
      }
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn btn-subtle" onClick={onClose}>
            Cancel
          </button>
          <button type="button" className="btn primary" disabled={!canRestore || busy} onClick={() => void confirm()}>
            {busy ? 'Working…' : 'Restore'}
          </button>
        </>
      }
    >
      <div className="ax-modal-form-stack policy-pack-layout">
        {error && <p className="page-toast-err">{error}</p>}
        <input
          type="file"
          accept=".zip,.ax-policy.zip,application/zip"
          onChange={(e) => void onFile(e.target.files?.[0] ?? null)}
        />
        {items.length > 0 && (
          <div className="policy-pack-split">
            <div className="page-table-wrap">
              <table className="page-table policy-pack-preview">
                <thead>
                  <tr>
                    <th>Kind</th>
                    <th>Id</th>
                    <th>Compare</th>
                    <th>Action</th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((item) => {
                    const key = `${item.kind}:${item.id}`;
                    const compare = item.compare ?? item.status;
                    const action = decisions[key] ?? (item.status === 'new' ? 'overwrite' : 'skip');
                    const labels = restoreDecisionLabels();
                    return (
                      <tr
                        key={key}
                        className={`policy-pack-preview-row${activeKey === key ? ' policy-pack-preview-row--active' : ''}`}
                        onClick={() => void openDiff(item)}
                      >
                        <td>{item.kind}</td>
                        <td>
                          <span className="policy-pack-item-title">{item.id}</span>
                          {item.summary ? (
                            <span className="policy-pack-item-desc">{item.summary}</span>
                          ) : (
                            <span className="policy-pack-item-desc">
                              {policyItemDescription({ id: item.id })}
                            </span>
                          )}
                        </td>
                        <td className="policy-pack-compare">
                          <span className={compareStatusClass(compare)}>{compareSummary(compare, item.newer)}</span>
                          {item.reason ? <span className="muted"> ({item.reason})</span> : null}
                        </td>
                        <td onClick={(e) => e.stopPropagation()}>
                          {item.status === 'invalid' ? (
                            <span className="muted">cannot install</span>
                          ) : (
                            <div className="policy-pack-action" role="group" aria-label={`Action for ${item.id}`}>
                              <button
                                type="button"
                                className="policy-pack-action-btn"
                                aria-pressed={action === 'skip'}
                                onClick={() => setDecisions({ ...decisions, [key]: 'skip' })}
                              >
                                {labels.reject}
                              </button>
                              <button
                                type="button"
                                className="policy-pack-action-btn"
                                aria-pressed={action === 'overwrite'}
                                onClick={() =>
                                  setDecisions({
                                    ...decisions,
                                    [key]: 'overwrite',
                                  })
                                }
                              >
                                {labels.accept}
                              </button>
                            </div>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            <aside className="policy-pack-inspect">
              {activeKey ? (
                <>
                  <h3 className="policy-pack-inspect-title">
                    {activeKey}
                    {diffCompare ? (
                      <>
                        {' '}
                        <span className={compareStatusClass(diffCompare)}>{compareSummary(diffCompare)}</span>
                      </>
                    ) : null}
                  </h3>
                  <UnifiedDiffView unified={diffUnified} compare={diffCompare} />
                </>
              ) : (
                <p className="muted">Click a row to compare local files with the package (git-style unified diff).</p>
              )}
            </aside>
          </div>
        )}
      </div>
    </ModalShell>
  );
}
