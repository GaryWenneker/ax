import { useEffect, useMemo, useState } from 'react';
import {
  deletePolicyRule,
  fetchPolicyRules,
  proposePolicyCapture,
  savePolicyCapture,
  setPolicyRuleEnabled,
} from '../policyApi';
import {
  DataTable,
  LevelBadge,
  ScopeBadge,
  PageCard,
  PageCardBody,
  PageEmpty,
  PageHero,
  PageLoading,
  PageShell,
  PageStack,
  PageToasts,
} from '../components/ui/PageLayout';
import {
  PolicyCount,
  PolicyRowActions,
  PolicyToolbar,
  SortTh,
} from '../components/ui/PolicyTable';
import { TagList } from '../components/PolicyMetaView';
import { LabelAutocomplete } from '../components/ui/LabelAutocomplete';
import {
  collectTags,
  filterRules,
  sortRules,
  toggleSort,
  type RuleSortKey,
  type SortDir,
} from '../components/ui/policyListUtils';
import { usePageContext } from '../context/UiContext';
import { POLICY_SCOPES, type CaptureProposal, type PolicyRuleRow } from '../policyTypes';

interface Props {
  onEdit: (id: string | null) => void;
  onMatch: () => void;
}

export default function PolicyRulesPage({ onEdit, onMatch }: Props) {
  const [rules, setRules] = useState<PolicyRuleRow[]>([]);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);
  const [captureOpen, setCaptureOpen] = useState(false);
  const [capturePrompt, setCapturePrompt] = useState('');
  const [captureProposal, setCaptureProposal] = useState<CaptureProposal | null>(null);
  const [captureError, setCaptureError] = useState('');
  const [captureLoading, setCaptureLoading] = useState(false);

  const [q, setQ] = useState('');
  const [level, setLevel] = useState('');
  const [scope, setScope] = useState('');
  const [tags, setTags] = useState<string[]>([]);
  const [always, setAlways] = useState('');
  const [sortKey, setSortKey] = useState<RuleSortKey>('priority');
  const [sortDir, setSortDir] = useState<SortDir>('desc');

  useEffect(() => {
    fetchPolicyRules()
      .then((r) => setRules(r.rules))
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  const tagOptions = useMemo(() => collectTags(rules), [rules]);

  const visible = useMemo(
    () => sortRules(filterRules(rules, { q, level, always, scope, tags }), sortKey, sortDir),
    [rules, q, level, always, scope, tags, sortKey, sortDir],
  );

  function toggleFilterTag(tag: string) {
    const key = tag.trim().toLowerCase();
    setTags((prev) =>
      prev.some((t) => t.toLowerCase() === key)
        ? prev.filter((t) => t.toLowerCase() !== key)
        : [...prev, tag],
    );
  }

  usePageContext('Rules', !loading && !error ? `${visible.length}/${rules.length} rules` : undefined);

  function setSort(key: RuleSortKey) {
    const next = toggleSort(sortKey, sortDir, key);
    setSortKey(next.key);
    setSortDir(next.dir);
  }

  async function remove(id: string) {
    if (!confirm(`Delete rule "${id}"?`)) return;
    try {
      await deletePolicyRule(id);
      setRules((prev) => prev.filter((r) => r.id !== id));
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to delete rule "${id}"`);
    }
  }

  async function toggleEnabled(id: string, enabled: boolean) {
    try {
      await setPolicyRuleEnabled(id, enabled);
      setRules((prev) => prev.map((r) => (r.id === id ? { ...r, enabled } : r)));
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to update rule "${id}"`);
    }
  }

  function openCapture() {
    setCaptureOpen(true);
    setCaptureProposal(null);
    setCaptureError('');
  }

  function closeCapture() {
    setCaptureOpen(false);
    setCapturePrompt('');
    setCaptureProposal(null);
    setCaptureError('');
  }

  async function runCapturePropose() {
    setCaptureLoading(true);
    setCaptureError('');
    setCaptureProposal(null);
    try {
      const result = await proposePolicyCapture(capturePrompt);
      if (!result.detected || !result.proposal) {
        setCaptureError('No directive detected. Try phrases like "je moet", "always", or prefix with @rule.');
        return;
      }
      setCaptureProposal(result.proposal);
    } catch (e) {
      setCaptureError(e instanceof Error ? e.message : 'Capture failed');
    } finally {
      setCaptureLoading(false);
    }
  }

  async function runCaptureSave() {
    if (!captureProposal) return;
    setCaptureLoading(true);
    setCaptureError('');
    try {
      const saved = await savePolicyCapture(captureProposal.frontmatter, captureProposal.body);
      const rulesRes = await fetchPolicyRules();
      setRules(rulesRes.rules);
      closeCapture();
      onEdit(saved.id);
    } catch (e) {
      setCaptureError(e instanceof Error ? e.message : 'Save failed');
    } finally {
      setCaptureLoading(false);
    }
  }

  if (loading) {
    return (
      <PageShell>
        <PageHero title="Rules" subtitle="Team policy rules injected into agent context." />
        <PageLoading label="Loading rules…" />
      </PageShell>
    );
  }

  return (
    <PageShell>
      <PageHero
        title="Rules"
        subtitle="Durable instructions matched by globs, triggers, or always-apply."
        actions={
          <>
            <button type="button" className="btn btn-subtle" onClick={onMatch}>Test match</button>
            <button type="button" className="btn btn-subtle" onClick={openCapture}>Capture</button>
            <button type="button" className="btn primary" onClick={() => onEdit(null)}>New rule</button>
          </>
        }
      />

      <PageToasts err={error || captureError || null} />

      <PageStack>
        {captureOpen && (
          <PageCard
            title="Capture from prompt"
            description='Paste a durable instruction. Preview first, then save.'
            footer={
              <>
                <button
                  type="button"
                  className="btn primary"
                  disabled={captureLoading || !capturePrompt.trim()}
                  onClick={runCapturePropose}
                >
                  {captureLoading && !captureProposal ? 'Analyzing…' : 'Preview rule'}
                </button>
                {captureProposal && (
                  <button
                    type="button"
                    className="btn primary"
                    disabled={captureLoading}
                    onClick={runCaptureSave}
                  >
                    {captureLoading ? 'Saving…' : 'Save rule'}
                  </button>
                )}
                <button type="button" className="btn btn-subtle" onClick={closeCapture}>Close</button>
              </>
            }
          >
            <PageCardBody>
              <div className="settings-row">
                <div className="settings-row-label">
                  <span className="settings-row-title">Prompt</span>
                </div>
                <div className="settings-row-control" style={{ alignItems: 'stretch' }}>
                  <textarea
                    className="settings-input"
                    value={capturePrompt}
                    onChange={(e) => setCapturePrompt(e.target.value)}
                    rows={3}
                    placeholder="je moet altijd dark mode gebruiken"
                    style={{ resize: 'vertical', minHeight: 72 }}
                  />
                </div>
              </div>
              {captureProposal && (
                <>
                  <div className="settings-divider" />
                  <div className="settings-subsection-label">Preview</div>
                  <div className="capture-meta muted" style={{ padding: '0 clamp(16px, 2vw, 28px) 8px' }}>
                    <span>id: {captureProposal.suggestedId}</span>
                    <span>confidence: {captureProposal.confidence}</span>
                  </div>
                  <pre className="page-code-block">{captureProposal.preview}</pre>
                </>
              )}
            </PageCardBody>
          </PageCard>
        )}

        <PageCard title="All rules" description="Filter with label autocomplete, then refine by level and layer.">
          <PolicyToolbar>
            <LabelAutocomplete
              options={tagOptions}
              selected={tags}
              onSelectedChange={setTags}
              query={q}
              onQueryChange={setQ}
              placeholder="Search id or add label…"
              ariaLabel="Filter rules by labels and text"
            />
            <select
              className="settings-select policy-toolbar-select"
              value={level}
              onChange={(e) => setLevel(e.target.value)}
              aria-label="Filter by level"
            >
              <option value="">All levels</option>
              <option value="CRITICAL">Critical</option>
              <option value="WARNING">Warning</option>
              <option value="INFO">Info</option>
            </select>
            <select
              className="settings-select policy-toolbar-select"
              value={scope}
              onChange={(e) => setScope(e.target.value)}
              aria-label="Filter by policy layer"
            >
              <option value="">All layers</option>
              {POLICY_SCOPES.map((s) => (
                <option key={s.value} value={s.value}>{s.label}</option>
              ))}
            </select>
            <select
              className="settings-select policy-toolbar-select"
              value={always}
              onChange={(e) => setAlways(e.target.value)}
              aria-label="Filter by always apply"
            >
              <option value="">Always apply: any</option>
              <option value="yes">Always apply</option>
              <option value="no">Conditional</option>
            </select>
            <PolicyCount shown={visible.length} total={rules.length} />
          </PolicyToolbar>

          <PageCardBody>
            {rules.length === 0 ? (
              <PageEmpty title="No rules yet">Create your first rule or capture one from a prompt.</PageEmpty>
            ) : visible.length === 0 ? (
              <PageEmpty title="No matching rules">Adjust your filters or search query.</PageEmpty>
            ) : (
              <DataTable dense>
                <thead>
                  <tr>
                    <SortTh label="ID" active={sortKey === 'id'} dir={sortDir} onClick={() => setSort('id')} />
                    <SortTh label="Level" active={sortKey === 'level'} dir={sortDir} onClick={() => setSort('level')} />
                    <SortTh label="Layer" active={sortKey === 'scope'} dir={sortDir} onClick={() => setSort('scope')} />
                    <SortTh label="Pri" active={sortKey === 'priority'} dir={sortDir} onClick={() => setSort('priority')} className="col-num" />
                    <th>Tags</th>
                    <th>Always</th>
                    <th>Enabled</th>
                    <SortTh label="Globs" active={sortKey === 'globs'} dir={sortDir} onClick={() => setSort('globs')} className="col-num" />
                    <SortTh label="Triggers" active={sortKey === 'triggers'} dir={sortDir} onClick={() => setSort('triggers')} className="col-num" />
                    <th className="col-actions">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {visible.map((r) => (
                    <tr key={r.id} className={`policy-table-row${r.enabled === false ? ' policy-table-row--disabled' : ''}`}>
                      <td className="mono">
                        <button type="button" className="policy-link" onClick={() => onEdit(r.id)}>
                          {r.id}
                        </button>
                      </td>
                      <td><LevelBadge level={r.level} /></td>
                      <td><ScopeBadge scope={r.scope} /></td>
                      <td className="num">{r.priority}</td>
                      <td className="policy-table-tags">
                        <TagList items={r.tags} onTagClick={toggleFilterTag} activeTags={tags} />
                      </td>
                      <td className="policy-table-flag">{r.alwaysApply ? 'yes' : '—'}</td>
                      <td>
                        <button
                          type="button"
                          className={`settings-toggle${r.enabled !== false ? ' on' : ''}`}
                          onClick={() => void toggleEnabled(r.id, r.enabled === false)}
                          aria-pressed={r.enabled !== false}
                          aria-label={r.enabled !== false ? `Disable ${r.id}` : `Enable ${r.id}`}
                          title={r.enabled !== false ? 'Enabled — click to disable' : 'Disabled — click to enable'}
                        >
                          <span className="settings-toggle-thumb" />
                        </button>
                      </td>
                      <td className="num">{r.globs.length}</td>
                      <td className="num">{r.triggers.length}</td>
                      <td className="col-actions">
                        <PolicyRowActions onEdit={() => onEdit(r.id)} onDelete={() => remove(r.id)} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </DataTable>
            )}
          </PageCardBody>
        </PageCard>
      </PageStack>
    </PageShell>
  );
}
