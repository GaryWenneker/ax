import { useEffect, useMemo, useRef, useState, type DragEvent } from 'react';
import ModalShell from './ModalShell';
import {
  diffPolicyPackageItem,
  downloadPolicyPackage,
  fetchPolicyRules,
  fetchPolicySkills,
  previewPolicyPackage,
  relocatePolicyItem,
  restorePolicyPackage,
  type PolicyPackageDiffHunk,
  type PolicyPackagePreviewItem,
} from '../policyApi';
import {
  compareStatusClass,
  compareSummary,
  emptyDiffCopy,
  pickDroppedPolicyZipFile,
  policyItemDescription,
  restoreDecisionLabels,
  hunkTakesForDecision,
  hunkTakeFromChecks,
  checksFromHunkTake,
  numberedHunkLines,
  setHunkTake,
  changeNavLabel,
  type RestoreDecisionValue,
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

function HunkDiffView({
  hunks,
  unified,
  compare,
  takes,
  activeIndex,
  onActiveIndex,
  onSetTake,
}: {
  hunks: PolicyPackageDiffHunk[];
  unified: string;
  compare: string;
  takes: Array<'local' | 'package' | 'both' | 'none'>;
  activeIndex: number;
  onActiveIndex: (i: number) => void;
  onSetTake: (index: number, take: 'local' | 'package' | 'both' | 'none') => void;
}) {
  if (hunks.length === 0) {
    return <UnifiedDiffView unified={unified} compare={compare} />;
  }
  return (
    <div className="policy-pack-hunks">
      {hunks.map((hunk) => {
        const isActive = hunk.index === activeIndex;
        const take = takes[hunk.index] ?? 'local';
        const checks = checksFromHunkTake(take);
        return (
          <section
            key={hunk.index}
            id={`policy-pack-hunk-${hunk.index}`}
            className={`policy-pack-hunk${isActive ? ' policy-pack-hunk--active' : ''}`}
            onClick={() => onActiveIndex(hunk.index)}
          >
            <div className="policy-pack-hunk-bar">
              <span className="policy-pack-hunk-title">Change {hunk.index + 1}</span>
            </div>
            <div className="policy-pack-hunk-cols">
              <div className="policy-pack-hunk-col">
                <label className="policy-pack-hunk-col-label">
                  <input
                    type="checkbox"
                    checked={checks.local}
                    onClick={(e) => e.stopPropagation()}
                    onChange={(e) => {
                      e.stopPropagation();
                      onSetTake(hunk.index, hunkTakeFromChecks(e.target.checked, checks.package));
                    }}
                  />
                  <span className="policy-pack-hunk-age">Old</span>
                  <span className="policy-pack-hunk-age-sub">local file</span>
                </label>
                <div className="policy-pack-hunk-pre" role="table" aria-label="Old (local file) lines">
                  {numberedHunkLines(hunk.local, hunk.localStart ?? 0).map((row, i) => (
                    <div key={i} className="policy-pack-hunk-line policy-pack-diff-line--del">
                      <span className="policy-pack-hunk-gutter">{row.n ?? ''}</span>
                      <span className="policy-pack-hunk-code">{row.text || ' '}</span>
                    </div>
                  ))}
                </div>
              </div>
              <div className="policy-pack-hunk-col">
                <label className="policy-pack-hunk-col-label">
                  <input
                    type="checkbox"
                    checked={checks.package}
                    onClick={(e) => e.stopPropagation()}
                    onChange={(e) => {
                      e.stopPropagation();
                      onSetTake(hunk.index, hunkTakeFromChecks(checks.local, e.target.checked));
                    }}
                  />
                  <span className="policy-pack-hunk-age">New</span>
                  <span className="policy-pack-hunk-age-sub">package</span>
                </label>
                <div className="policy-pack-hunk-pre" role="table" aria-label="New (package) lines">
                  {numberedHunkLines(hunk.package, hunk.packageStart ?? 0).map((row, i) => (
                    <div key={i} className="policy-pack-hunk-line policy-pack-diff-line--add">
                      <span className="policy-pack-hunk-gutter">{row.n ?? ''}</span>
                      <span className="policy-pack-hunk-code">{row.text || ' '}</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </section>
        );
      })}
      <div className="policy-pack-hunk-nav">
        <button
          type="button"
          className="btn btn-subtle"
          disabled={activeIndex <= 0}
          onClick={() => onActiveIndex(Math.max(0, activeIndex - 1))}
        >
          Previous
        </button>
        <span>{changeNavLabel(activeIndex, hunks.length)}</span>
        <button
          type="button"
          className="btn btn-subtle"
          disabled={activeIndex >= hunks.length - 1}
          onClick={() => onActiveIndex(Math.min(hunks.length - 1, activeIndex + 1))}
        >
          Next
        </button>
      </div>
    </div>
  );
}

function ComposeModal({ onClose }: { onClose: () => void }) {
  const [name, setName] = useState('Team pack');
  const [description, setDescription] = useState('');
  const [rules, setRules] = useState<PolicyRuleRow[]>([]);
  const [skills, setSkills] = useState<PolicySkillRow[]>([]);
  const [ruleIds, setRuleIds] = useState<Set<string>>(new Set());
  const [skillNames, setSkillNames] = useState<Set<string>>(new Set());
  const [includeGlobal, setIncludeGlobal] = useState(false);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [inspect, setInspect] = useState<{ title: string; description: string; body: string } | null>(null);

  useEffect(() => {
    void Promise.all([fetchPolicyRules(), fetchPolicySkills()])
      .then(([r, s]) => {
        setRules(r.rules.filter((x) => (includeGlobal || x.origin !== 'global') && isShareablePolicyItem(x.scope, x.enabled !== false)));
        setSkills(s.skills.filter((x) => (includeGlobal || x.origin !== 'global') && isShareablePolicyItem(x.scope, x.enabled !== false)));
      })
      .catch((e) => setError(e instanceof Error ? e.message : 'Failed to load policy'));
  }, [includeGlobal]);

  const canDownload = name.trim().length > 0 && (ruleIds.size > 0 || skillNames.size > 0);
  const ruleIdList = rules.map((r) => r.id);
  const skillNameList = skills.map((s) => s.name);

  async function download() {
    setBusy(true);
    setError('');
    try {
      const packRuleIds = rules.filter((r) => ruleIds.has(r.id) && r.origin !== 'global').map((r) => r.id);
      const packSkillNames = skills.filter((s) => skillNames.has(s.name) && s.origin !== 'global').map((s) => s.name);
      const skipped = ruleIds.size + skillNames.size - packRuleIds.length - packSkillNames.length;
      if (packRuleIds.length === 0 && packSkillNames.length === 0) {
        throw new Error(
          skipped
            ? 'Selected items live only in global.db. Move them to this project to pack, or include project items.'
            : 'Select at least one project item',
        );
      }
      const { blob, filename } = await downloadPolicyPackage({
        name: name.trim(),
        description: description.trim(),
        ruleIds: packRuleIds,
        skillNames: packSkillNames,
      });
      downloadBlob(blob, filename);
      if (skipped > 0) {
        setError(`Packed project items. Skipped ${skipped} global.db cop${skipped === 1 ? 'y' : 'ies'} (not on disk).`);
        return;
      }
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
        <label className="settings-field policy-pack-include-global">
          <input type="checkbox" checked={includeGlobal} onChange={(e) => setIncludeGlobal(e.target.checked)} />
          <span>Include global.db copies in this list (pack still writes project files only)</span>
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
  const [decisions, setDecisions] = useState<Record<string, RestoreDecisionValue>>({});
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const [diffUnified, setDiffUnified] = useState('');
  const [diffCompare, setDiffCompare] = useState('');
  const [diffNewer, setDiffNewer] = useState<string | null>(null);
  const [diffHunks, setDiffHunks] = useState<PolicyPackageDiffHunk[]>([]);
  const [activeHunk, setActiveHunk] = useState(0);
  const [dragOver, setDragOver] = useState(false);
  const [copyToGlobal, setCopyToGlobal] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const canRestore = useMemo(
    () => file && items.some((i) => i.status !== 'invalid'),
    [file, items],
  );

  useEffect(() => {
    if (!activeKey || diffHunks.length === 0) return;
    document.getElementById(`policy-pack-hunk-${activeHunk}`)?.scrollIntoView({ block: 'nearest' });
  }, [activeHunk, activeKey, diffHunks.length]);

  async function onFile(f: File | null) {
    setFile(f);
    setItems([]);
    setDecisions({});
    setError('');
    setActiveKey(null);
    setDiffUnified('');
    setDiffCompare('');
    setDiffNewer(null);
    setDiffHunks([]);
    setActiveHunk(0);
    if (!f) return;
    setBusy(true);
    try {
      const preview = await previewPolicyPackage(f);
      setPackName(preview.name);
      setItems(preview.items);
      const next: Record<string, RestoreDecisionValue> = {};
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

  function onDropZoneLeave(e: DragEvent) {
    if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node | null)) {
      setDragOver(false);
    }
  }

  function onDropZoneDrop(e: DragEvent) {
    e.preventDefault();
    setDragOver(false);
    const zip = pickDroppedPolicyZipFile(e.dataTransfer.files);
    if (!zip) {
      setError('Drop an .ax-policy.zip file.');
      return;
    }
    void onFile(zip);
  }

  async function openDiff(item: PolicyPackagePreviewItem) {
    if (!file) return;
    const key = `${item.kind}:${item.id}`;
    setActiveKey(key);
    setError('');
    try {
      const diff = await diffPolicyPackageItem(file, item.kind, item.id);
      setDiffCompare(diff.compare);
      setDiffNewer(item.newer ?? null);
      setDiffUnified(diff.unified);
      setDiffHunks(diff.hunks ?? []);
      setActiveHunk(0);
    } catch (e) {
      setDiffUnified('');
      setDiffCompare(item.compare ?? item.status);
      setDiffNewer(item.newer ?? null);
      setDiffHunks([]);
      setError(e instanceof Error ? e.message : 'Diff failed');
    }
  }

  async function confirm() {
    if (!file) return;
    setBusy(true);
    setError('');
    try {
      const result = await restorePolicyPackage(file, decisions);
      if (copyToGlobal) {
        for (const path of result.written) {
          const skill = /skills\/([^/]+)\//.exec(path);
          const rule = /rules\/([^/]+)\.mdc$/.exec(path);
          if (skill) await relocatePolicyItem('skill', skill[1], 'global');
          if (rule) await relocatePolicyItem('rule', rule[1], 'global');
        }
      }
      onRestored();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Restore failed');
    } finally {
      setBusy(false);
    }
  }

  return (
    <ModalShell
      size={items.length > 0 ? 'full' : 'md'}
      title="Restore package"
      subtitle={
        packName
          ? `Preview: ${packName}. Click a row to inspect changes. Accept or reject each hunk.`
          : 'Drop an .ax-policy.zip here, or choose a file. Then Accept or Reject each item, or individual changes in the diff. Local newer files default to Reject.'
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
      <div className={`ax-modal-form-stack${items.length > 0 ? ' policy-pack-layout' : ''}`}>
        {error && <p className="page-toast-err">{error}</p>}
        <label className="settings-field policy-pack-include-global">
          <input type="checkbox" checked={copyToGlobal} onChange={(e) => setCopyToGlobal(e.target.checked)} />
          <span>Also copy restored items into global.db</span>
        </label>
        <div
          className={`policy-pack-drop${items.length > 0 ? ' policy-pack-drop--compact' : ''}${dragOver ? ' policy-pack-drop--active' : ''}`}
          onDragEnter={(e) => {
            e.preventDefault();
            setDragOver(true);
          }}
          onDragOver={(e) => {
            e.preventDefault();
            e.dataTransfer.dropEffect = 'copy';
          }}
          onDragLeave={onDropZoneLeave}
          onDrop={onDropZoneDrop}
        >
          <input
            ref={fileInputRef}
            id="policy-pack-zip-input"
            className="policy-pack-drop-input"
            type="file"
            accept=".zip,.ax-policy.zip,application/zip"
            onChange={(e) => void onFile(e.target.files?.[0] ?? null)}
          />
          {items.length === 0 ? (
            <label className="policy-pack-drop-label" htmlFor="policy-pack-zip-input">
              Drop an .ax-policy.zip here, or choose a file
            </label>
          ) : (
            <>
              <span className="policy-pack-drop-file">{file?.name}</span>
              <button type="button" className="btn btn-subtle" onClick={() => fileInputRef.current?.click()}>
                Choose another file
              </button>
            </>
          )}
        </div>
        {items.length > 0 && (
          <div className={`policy-pack-split${activeKey ? ' policy-pack-split--with-diff' : ''}`}>
            <div className="page-table-wrap policy-pack-preview-wrap">
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
                    const fileLabel =
                      action === 'overwrite' ? 'accept' : action === 'skip' ? 'reject' : 'partial';
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
                          <span className={compareStatusClass(compare, item.newer)}>{compareSummary(compare, item.newer)}</span>
                          {item.reason ? <span className="muted"> ({item.reason})</span> : null}
                        </td>
                        <td onClick={(e) => e.stopPropagation()}>
                          {item.status === 'invalid' ? (
                            <span className="muted">cannot install</span>
                          ) : (
                            <div className="policy-pack-action-wrap">
                              <div className="policy-pack-action" role="group" aria-label={`Action for ${item.id}`}>
                                <button
                                  type="button"
                                  className="policy-pack-action-btn"
                                  aria-pressed={fileLabel === 'reject'}
                                  onClick={() => setDecisions({ ...decisions, [key]: 'skip' })}
                                >
                                  {labels.reject}
                                </button>
                                <button
                                  type="button"
                                  className="policy-pack-action-btn"
                                  aria-pressed={fileLabel === 'accept'}
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
                              {fileLabel === 'partial' ? (
                                <span
                                  className="policy-pack-partial"
                                  title="Mixed hunks: some Old, some New"
                                >
                                  Partial
                                </span>
                              ) : null}
                            </div>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            {activeKey ? (
              <aside className="policy-pack-inspect">
                <h3 className="policy-pack-inspect-title">
                  {activeKey}
                  {diffCompare ? (
                    <>
                      {' '}
                      <span className={compareStatusClass(diffCompare, diffNewer)}>{compareSummary(diffCompare, diffNewer)}</span>
                    </>
                  ) : null}
                </h3>
                <HunkDiffView
                  hunks={diffHunks}
                  unified={diffUnified}
                  compare={diffCompare}
                  takes={hunkTakesForDecision(decisions[activeKey] ?? 'skip', diffHunks.length)}
                  activeIndex={activeHunk}
                  onActiveIndex={setActiveHunk}
                  onSetTake={(index, take) => {
                    const next = setHunkTake(
                      decisions[activeKey] ?? 'skip',
                      index,
                      diffHunks.length,
                      take,
                    );
                    setDecisions({ ...decisions, [activeKey]: next });
                  }}
                />
              </aside>
            ) : (
              <p className="muted">Click a row to compare local files with the package. Accept or reject each change.</p>
            )}
          </div>
        )}
      </div>
    </ModalShell>
  );
}
