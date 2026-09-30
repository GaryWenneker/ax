import { resolveOpenTarget } from '../lib/policySelection';
import { useLive, useNewKeys } from '../lib/useLive';
import { useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';
import {
  deletePolicyCopy,
  deletePolicyRule,
  fetchPolicyRules,
  fetchPolicySyncSettings,
  proposePolicyCapture,
  relocatePolicyItem,
  savePolicyCapture,
  savePolicySyncSettings,
  setPolicyRuleEnabled,
  setPolicyRuleStorage,
} from '../policyApi';
import {
  PageCard,
  PageCardBody,
  PageEmpty,
  PageHero,
  PageLoading,
  PageShell,
  PageStack,
  PageToasts,
} from '../components/ui/PageLayout';
import { PolicyCount, PolicySortControl, PolicyToolbar } from '../components/ui/PolicyTable';
import { PolicyCalmList } from '../components/ui/PolicyCalmList';
import { PolicyContextMenu } from '../components/ui/PolicyContextMenu';
import { PolicyDbLegend } from '../components/PolicyMetaView';
import { priorityMeta, ruleRowSubtitle } from '../lib/calmRows';
import PolicyRuleInlineWorkspace from '../components/PolicyRuleInlineWorkspace';
import { PolicyListResizeHandle } from '../components/PolicyEditorResize';
import PolicyZipPackageButtons from '../components/PolicyZipPackageModals';
import { LabelAutocomplete } from '../components/ui/LabelAutocomplete';
import {
  collectTags,
  filterRules,
  hydratePolicyListItem,
  isGlobalPolicy,
  normalizePolicyScope,
  policyOverviewMenuItems,
  sortRules,
  type PolicyMenuId,
  type RuleSortKey,
  type SortDir,
} from '../components/ui/policyListUtils';
import { usePageContext } from '../context/UiContext';
import { POLICY_SCOPES, type CaptureProposal, type PolicyRuleRow } from '../policyTypes';
import { visibleRuleGroups, toggleCollapsed, ruleResolvedGroup } from '../skillGroups';
import {
  allListedCollapsed,
  allListedExpanded,
  collapseAllGroupIds,
  expandAllGroupIds,
  matchesGroupFilter,
} from '../skillGroupFilter';
import { PolicyGroupListControls } from '../components/ui/PolicyGroupListControls';
import { loadJson, saveJson } from '../lib/uiStorage';
import AutoGroupModal from '../components/AutoGroupModal';
import PolicyGraphOverlay from '../components/PolicyGraphOverlay';
import { menuTargets, nextRowSelection, toggleVisibleSelection } from '../lib/policySelection';
import {
  POLICY_BLADE_DISMISS_MS,
  policyDetailOpen,
  policyWorkspaceHostClass,
  selectionHidden,
} from '../lib/policyBladeMotion';

const RULE_SORT_OPTIONS: Array<{ key: RuleSortKey; label: string }> = [
  { key: 'id', label: 'ID' },
  { key: 'level', label: 'Level' },
  { key: 'scope', label: 'Layer' },
  { key: 'priority', label: 'Priority' },
  { key: 'globs', label: 'Globs' },
  { key: 'triggers', label: 'Triggers' },
];

interface Props {
  selectedId: string | null;
  /** `origin`/`projectId` from the URL: which copy of the item to open. */
  origin?: string | null;
  projectId?: number | null;
  onSelect: (id: string | null, origin?: string, projectId?: number) => void;
  onEditFull: (id: string | null, origin?: string, projectId?: number) => void;
  onMatch: () => void;
}

export default function PolicyRulesPage({ selectedId: selectedIdFromRoute, origin: routeOrigin, projectId: routeProjectId, onSelect, onEditFull, onMatch }: Props) {
  const [selectedId, setSelectedId] = useState<string | null>(selectedIdFromRoute);
  const [graphOpen, setGraphOpen] = useState(false);
  const [autoGroupOpen, setAutoGroupOpen] = useState(false);
  const [openOrigin, setOpenOrigin] = useState<string | undefined>();
  const [openProjectId, setOpenProjectId] = useState<number | undefined>();
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set(selectedIdFromRoute ? [selectedIdFromRoute] : []));
  const [anchorId, setAnchorId] = useState<string | null>(selectedIdFromRoute);
  const [bladeClosing, setBladeClosing] = useState(false);
  const editorRef = useRef<HTMLDivElement | null>(null);
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;

  useEffect(() => {
    setSelectedId(selectedIdFromRoute);
    if (selectedIdFromRoute) {
      setSelectedIds(new Set([selectedIdFromRoute]));
      setAnchorId(selectedIdFromRoute);
    }
  }, [selectedIdFromRoute]);


  function selectRule(id: string | null, row?: { origin?: string; projectId?: number }) {
    setBladeClosing(false);
    setSelectedId(id);
    const global = row && isGlobalPolicy(row);
    onSelectRef.current(id, global ? 'global' : undefined, global ? row.projectId : undefined);
    if (row) {
      setOpenOrigin(row.origin);
      setOpenProjectId(row.projectId);
    }
    if (id) {
      setSelectedIds(new Set([id]));
      setAnchorId(id);
    } else if (selectedIds.size <= 1) {
      setSelectedIds(new Set());
    }
  }

  function requestCloseBlade() {
    if (!selectedId || bladeClosing) return;
    setBladeClosing(true);
  }

  useEffect(() => {
    if (!bladeClosing) return;
    const t = window.setTimeout(() => {
      setBladeClosing(false);
      setSelectedId(null);
      onSelectRef.current(null);
      setSelectedIds((prev) => (prev.size <= 1 ? new Set() : prev));
    }, POLICY_BLADE_DISMISS_MS);
    return () => window.clearTimeout(t);
  }, [bladeClosing]);
  const [rules, setRules] = useState<PolicyRuleRow[]>([]);

  useEffect(() => {
    if (!selectedIdFromRoute) return;
    const t = resolveOpenTarget(
      rules.map((r) => ({ key: r.id, origin: r.origin, projectId: r.projectId })),
      selectedIdFromRoute,
      routeOrigin,
      routeProjectId,
    );
    setOpenOrigin(t.origin);
    setOpenProjectId(t.projectId);
  }, [selectedIdFromRoute, routeOrigin, routeProjectId, rules]);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; row: PolicyRuleRow } | null>(null);
  const [captureOpen, setCaptureOpen] = useState(false);
  const [capturePrompt, setCapturePrompt] = useState('');
  const [captureProposal, setCaptureProposal] = useState<CaptureProposal | null>(null);
  const [captureError, setCaptureError] = useState('');
  const [captureLoading, setCaptureLoading] = useState(false);

  const [q, setQ] = useState('');
  const [level, setLevel] = useState('');
  const [scope, setScope] = useState('');
  const [origin, setOrigin] = useState('');
  const [tags, setTags] = useState<string[]>([]);
  const [always, setAlways] = useState('');
  const [sortKey, setSortKey] = useState<RuleSortKey>('id');
  const [sortDir, setSortDir] = useState<SortDir>('asc');
  const [projectStorage, setProjectStorage] = useState<'files' | 'database'>('files');
  const [collapsed, setCollapsed] = useState<Set<string>>(
    () => new Set(loadJson<string[]>('rules-groups-collapsed', [])),
  );
  const [groupIds, setGroupIds] = useState<string[]>(
    () => loadJson<string[]>('rules-groups-filter', []),
  );

  function persistCollapsed(next: Set<string>) {
    saveJson('rules-groups-collapsed', [...next]);
    setCollapsed(next);
  }

  function persistGroupIds(next: string[]) {
    saveJson('rules-groups-filter', next);
    setGroupIds(next);
  }

  function toggleGroup(id: string) {
    persistCollapsed(toggleCollapsed(collapsed, id));
  }

  useEffect(() => {
    Promise.all([fetchPolicyRules(), fetchPolicySyncSettings()])
      .then(([r, s]) => {
        setRules(r.rules);
        setProjectStorage(s.storage === 'database' ? 'database' : 'files');
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  function reloadRules() {
    fetchPolicyRules()
      .then((r) => setRules(r.rules))
      .catch((e: Error) => setError(e.message));
  }

  useLive('rules', reloadRules);

  const listed = useMemo(
    () =>
      rules.map(
        (r) =>
          hydratePolicyListItem(r as unknown as Record<string, unknown>) as unknown as PolicyRuleRow,
      ),
    [rules],
  );

  const fresh = useNewKeys(loading ? null : listed.map((r) => r.rowKey ?? r.id));

  const tagOptions = useMemo(() => collectTags(listed), [listed]);

  const searchVisible = useMemo(
    () => sortRules(filterRules(listed, { q, level, always, scope, tags, origin }), sortKey, sortDir),
    [listed, q, level, always, scope, tags, origin, sortKey, sortDir],
  );

  const groupOptions = useMemo(
    () =>
      visibleRuleGroups(searchVisible).map((g) => ({
        id: g.id,
        label: g.label,
        count: g.rules.length,
      })),
    [searchVisible],
  );

  const visible = useMemo(
    () => searchVisible.filter((r) => matchesGroupFilter(ruleResolvedGroup(r), groupIds)),
    [searchVisible, groupIds],
  );

  const grouped = useMemo(() => visibleRuleGroups(visible), [visible]);
  const listedIds = useMemo(() => grouped.map((g) => g.id), [grouped]);
  const visibleRowIds = useMemo(() => visible.map((r) => r.id), [visible]);
  const ruleById = useMemo(() => new Map(visible.map((r) => [r.id, r])), [visible]);

  useEffect(() => {
    function onPtr(e: PointerEvent) {
      if (!selectedId) return;
      const t = e.target as Node | null;
      if (!t) return;
      if (editorRef.current?.contains(t)) return;
      if ((t as HTMLElement).closest?.('.policy-context-menu')) return;
      if ((t as HTMLElement).closest?.('.policy-calm-row')) return;
      if ((t as HTMLElement).closest?.('.policy-list-resize-handle')) return;
      requestCloseBlade();
    }
    document.addEventListener('pointerdown', onPtr);
    return () => document.removeEventListener('pointerdown', onPtr);
  }, [selectedId, bladeClosing]);

  function onRuleRowClick(e: MouseEvent, r: PolicyRuleRow) {
    const next = nextRowSelection({
      id: r.id,
      visibleIds: visibleRowIds,
      selected: selectedIds,
      anchor: anchorId,
      metaKey: e.metaKey || e.ctrlKey,
      shiftKey: e.shiftKey,
    });
    setSelectedIds(next.selected);
    setAnchorId(next.anchor);
    if (next.selected.size === 0 && selectedId === r.id) {
      requestCloseBlade();
      return;
    }
    setBladeClosing(false);
    setSelectedId(next.openId);
    const openRow = next.openId === r.id ? r : next.openId ? rules.find((x) => x.id === next.openId) : undefined;
    const openGlobal = openRow && isGlobalPolicy(openRow);
    onSelectRef.current(next.openId, openGlobal ? 'global' : undefined, openGlobal ? openRow.projectId : undefined);
    if (next.openId) {
      setOpenOrigin(r.origin);
      setOpenProjectId(r.projectId);
    }
  }

  function onRuleContext(e: MouseEvent, r: PolicyRuleRow) {
    e.preventDefault();
    if (!selectedIds.has(r.id)) {
      setSelectedIds(new Set([r.id]));
      setAnchorId(r.id);
    }
    setCtxMenu({ x: e.clientX, y: e.clientY, row: r });
  }

  function toggleFilterTag(tag: string) {
    const key = tag.trim().toLowerCase();
    setTags((prev) =>
      prev.some((t) => t.toLowerCase() === key)
        ? prev.filter((t) => t.toLowerCase() !== key)
        : [...prev, tag],
    );
  }

  function toggleFilterLevel(lvl: string) {
    setLevel((prev) => (prev === lvl ? '' : lvl));
  }

  function toggleFilterScope(rawScope?: string) {
    const normalized = normalizePolicyScope(rawScope);
    setScope((prev) => (prev === normalized ? '' : normalized));
  }

  useEffect(() => {
    if (selectionHidden(selectedId, visible.map((r) => r.id), loading)) {
      selectRule(null);
    }
  }, [selectedId, visible, loading]);

  usePageContext('Rules', !loading && !error ? `${visible.length}/${listed.length} rules` : undefined);

  async function remove(id: string) {
    if (!confirm(`Delete rule "${id}"?`)) return;
    try {
      await deletePolicyRule(id);
      setRules((prev) => prev.filter((r) => r.id !== id));
      if (selectedId === id) selectRule(null);
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

  async function onPolicyMenu(action: PolicyMenuId) {
    const r = ctxMenu?.row;
    if (!r) return;
    const targets = menuTargets(r, selectedIds, listed, (row) => row.id);
    if (action === 'delete') {
      const ok = isGlobalPolicy(r)
        ? confirm(`Delete ${targets.length} item(s) from global.db?`)
        : confirm(`Delete ${targets.length} rule(s)?`);
      if (!ok) return;
    }
    try {
      for (const row of targets) {
        if (action === 'open' && targets.length === 1) {
          selectRule(row.id, row);
        }
        if (action === 'edit' && targets.length === 1) onEditFull(row.id, row.origin, row.projectId);
        if (action === 'enable' && !isGlobalPolicy(row)) await toggleEnabled(row.id, true);
        if (action === 'disable' && !isGlobalPolicy(row)) await toggleEnabled(row.id, false);
        if (action === 'move-global' && !isGlobalPolicy(row)) {
          await relocatePolicyItem('rule', row.id, 'global', row.projectId);
        }
        if (action === 'move-project' && isGlobalPolicy(row)) {
          await relocatePolicyItem('rule', row.id, 'project', row.projectId);
        }
        if (action === 'delete') {
          if (isGlobalPolicy(row)) await deletePolicyCopy('rule', row.id, row.projectId);
          else {
            await deletePolicyRule(row.id);
            setRules((prev) => prev.filter((x) => x.id !== row.id));
          }
        }
      }
      if (action === 'move-global' || action === 'move-project' || (action === 'delete' && targets.some(isGlobalPolicy))) {
        reloadRules();
      }
      if (action === 'move-global' || action === 'delete') {
        if (targets.some((t) => t.id === selectedId)) selectRule(null);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Context menu action failed');
    }
  }

  async function toggleProjectStorage(next: 'files' | 'database') {
    try {
      const s = await savePolicySyncSettings({ storage: next });
      setProjectStorage(s.storage === 'database' ? 'database' : 'files');
      reloadRules();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to update project storage default');
    }
  }

  async function toggleItemStorage(id: string, currentEffective?: string) {
    const next = currentEffective === 'database' ? 'files' : 'database';
    try {
      const res = await setPolicyRuleStorage(id, next);
      setRules((prev) =>
        prev.map((r) =>
          r.id === id
            ? {
                ...r,
                storage: next,
                effectiveStorage: res.effectiveStorage ?? next,
                storageIsOverride: res.storageIsOverride ?? true,
              }
            : r,
        ),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to update storage for "${id}"`);
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
      selectRule(saved.id);
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
            <label className="policy-storage-default" title="Project default storage (per-item overrides keep their own mode)">
              <span className="muted">Default</span>
              <span className={`policy-storage-label${projectStorage === 'files' ? ' active' : ''}`}>MD</span>
              <button
                type="button"
                role="switch"
                aria-checked={projectStorage === 'database'}
                aria-label="Project default storage"
                className={`settings-toggle${projectStorage === 'database' ? ' on' : ''}`}
                onClick={() => void toggleProjectStorage(projectStorage === 'database' ? 'files' : 'database')}
              >
                <span className="settings-toggle-thumb" />
              </button>
              <span className={`policy-storage-label${projectStorage === 'database' ? ' active' : ''}`}>DB</span>
            </label>
            <button type="button" className="btn btn-subtle" onClick={() => setGraphOpen(true)}>Graph</button>
            <button type="button" className="btn btn-subtle" onClick={() => setAutoGroupOpen(true)}>Auto-group</button>
            {autoGroupOpen && (
              <AutoGroupModal
                kind="rule"
                rows={rules}
                onClose={() => setAutoGroupOpen(false)}
                onApplied={() => {
                  setAutoGroupOpen(false);
                  void reloadRules();
                }}
              />
            )}
            {graphOpen && (
              <PolicyGraphOverlay
                currentKey={selectedId ? `rule:${openOrigin === 'global' ? 'global' : 'project'}:${selectedId}` : null}
                onClose={() => setGraphOpen(false)}
              />
            )}
            <button type="button" className="btn btn-subtle" onClick={onMatch}>Test match</button>
            <PolicyZipPackageButtons onRestored={() => void reloadRules()} />
            <button type="button" className="btn btn-subtle" onClick={openCapture}>Capture</button>
            <button type="button" className="btn primary" onClick={() => onEditFull(null)}>New rule</button>
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

        <PageCard title="All rules" description="Grouped by catalog folders. Empty groups stay hidden until a rule is assigned.">
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
              value={origin}
              onChange={(e) => setOrigin(e.target.value)}
              aria-label="Filter by database"
            >
              <option value="">All databases</option>
              <option value="project">This project (ax.db)</option>
              <option value="global">Other projects (global.db)</option>
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
            <PolicySortControl
              options={RULE_SORT_OPTIONS}
              sortKey={sortKey}
              sortDir={sortDir}
              onKey={(key) => setSortKey(key)}
              onDir={() => setSortDir(sortDir === 'asc' ? 'desc' : 'asc')}
            />
            <label className="calm-select-all">
              <input
                type="checkbox"
                aria-label="Select all visible rules"
                checked={visibleRowIds.length > 0 && visibleRowIds.every((id) => selectedIds.has(id))}
                onChange={() => setSelectedIds(toggleVisibleSelection(visibleRowIds, selectedIds))}
              />
              <span className="muted">All</span>
            </label>
            <PolicyGroupListControls
              options={groupOptions}
              selectedIds={groupIds}
              onSelectedIds={persistGroupIds}
              onCollapseAll={() => persistCollapsed(collapseAllGroupIds(listedIds))}
              onExpandAll={() => persistCollapsed(expandAllGroupIds())}
              collapseAllDisabled={allListedCollapsed(listedIds, collapsed)}
              expandAllDisabled={allListedExpanded(listedIds, collapsed)}
            />
            <PolicyCount shown={visible.length} total={listed.length} />
            <PolicyDbLegend />
          </PolicyToolbar>

          <PageCardBody>
            {listed.length === 0 ? (
              <PageEmpty title="No rules yet">Create your first rule or capture one from a prompt.</PageEmpty>
            ) : visible.length === 0 ? (
              <PageEmpty title="No matching rules">Adjust your filters or search query.</PageEmpty>
            ) : (
              <div className={`page-split policy-rules-split${policyDetailOpen(selectedId, bladeClosing) ? ' page-split--with-detail' : ''}`}>
                <div className="page-split-main">
                  <PolicyCalmList
                    groups={grouped.map((g) => ({
                      id: g.id,
                      label: g.label,
                      items: g.rules.map((r) => ({
                        key: r.rowKey ?? r.id,
                        id: r.id,
                        subtitle: ruleRowSubtitle(r, projectStorage),
                        meta: priorityMeta(r.priority),
                        level: r.level,
                        scope: r.scope,
                        origin: r.origin,
                        projectName: r.projectName,
                        tags: r.tags ?? [],
                        enabled: r.enabled !== false,
                        global: isGlobalPolicy(r),
                        storage: (r.effectiveStorage ?? projectStorage) === 'database' ? 'database' : 'files',
                      })),
                    }))}
                    fresh={fresh}
                    collapsed={collapsed}
                    onToggleGroup={toggleGroup}
                    selectedIds={selectedIds}
                    compact={Boolean(selectedId)}
                    filters={{ level, scope, tags, onLevel: toggleFilterLevel, onScope: toggleFilterScope, onTag: toggleFilterTag }}
                    onRowClick={(e, id) => {
                      const r = ruleById.get(id);
                      if (r) onRuleRowClick(e, r);
                    }}
                    onRowContext={(e, id) => {
                      const r = ruleById.get(id);
                      if (r) onRuleContext(e, r);
                    }}
                    onCheck={(id, checked) => {
                      const next = new Set(selectedIds);
                      if (checked) next.add(id);
                      else next.delete(id);
                      setSelectedIds(next);
                      setAnchorId(id);
                    }}
                    onToggleEnabled={(id, enabled) => void toggleEnabled(id, enabled)}
                    onToggleStorage={(id, current) => void toggleItemStorage(id, current)}
                    onEdit={(id) => {
                      const r = ruleById.get(id);
                      onEditFull(id, r?.origin, r?.projectId);
                    }}
                    onDelete={(id) => {
                      const r = ruleById.get(id);
                      if (r && isGlobalPolicy(r)) {
                        if (confirm(`Delete rule "${id}" from global.db?`)) {
                          void deletePolicyCopy('rule', id, r.projectId).then(reloadRules);
                        }
                        return;
                      }
                      void remove(id);
                    }}
                  />
                </div>
                {policyDetailOpen(selectedId, bladeClosing) && selectedId && (routeOrigin || !loading) ? (
                  <>
                    <PolicyListResizeHandle />
                    <div ref={editorRef} className={policyWorkspaceHostClass(bladeClosing)}>
                      <PolicyRuleInlineWorkspace
                        ruleId={selectedId}
                        origin={openOrigin}
                        projectId={openProjectId}
                        onClose={() => requestCloseBlade()}
                        onSaved={reloadRules}
                      />
                    </div>
                  </>
                ) : null}
              </div>
            )}
          </PageCardBody>
        </PageCard>
      </PageStack>
      {ctxMenu ? (
        <PolicyContextMenu
          x={ctxMenu.x}
          y={ctxMenu.y}
          items={policyOverviewMenuItems(ctxMenu.row.origin, ctxMenu.row.enabled)}
          onPick={onPolicyMenu}
          onClose={() => setCtxMenu(null)}
        />
      ) : null}
    </PageShell>
  );
}
