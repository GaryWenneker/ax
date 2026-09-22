import { useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';
import {
  deletePolicyCopy,
  deletePolicySkill,
  fetchPolicySkills,
  fetchPolicySyncSettings,
  relocatePolicyItem,
  savePolicySyncSettings,
  setPolicySkillEnabled,
  setPolicySkillStorage,
} from '../policyApi';
import {
  DataTable,
  PageCard,
  PageCardBody,
  PageEmpty,
  PageHero,
  PageLoading,
  PageShell,
  PageStack,
  PageToasts,
  ScopeBadge,
} from '../components/ui/PageLayout';
import { TagList, OriginDbBadge, PolicyDbLegend } from '../components/PolicyMetaView';
import PolicySkillInlineWorkspace from '../components/PolicySkillInlineWorkspace';
import { PolicyListResizeHandle } from '../components/PolicyEditorResize';
import PolicyZipPackageButtons from '../components/PolicyZipPackageModals';
import { LabelAutocomplete } from '../components/ui/LabelAutocomplete';
import {
  PolicyCount,
  PolicyRowActions,
  PolicyToolbar,
  SortTh,
} from '../components/ui/PolicyTable';
import { PolicyContextMenu } from '../components/ui/PolicyContextMenu';
import {
  collectTags,
  filterSkills,
  hydratePolicyListItem,
  isGlobalPolicy,
  normalizePolicyScope,
  policyDbRowStyle,
  policyOverviewMenuItems,
  sortSkills,
  toggleSort,
  type PolicyMenuId,
  type SkillSortKey,
  type SortDir,
} from '../components/ui/policyListUtils';
import { usePageContext } from '../context/UiContext';
import { POLICY_SCOPES, type PolicySkillRow } from '../policyTypes';
import { visibleSkillGroups, toggleCollapsed, skillResolvedGroup } from '../skillGroups';
import {
  allListedCollapsed,
  allListedExpanded,
  collapseAllGroupIds,
  expandAllGroupIds,
  matchesGroupFilter,
} from '../skillGroupFilter';
import { PolicyGroupListControls } from '../components/ui/PolicyGroupListControls';
import { GitShareDot } from '../components/ui/GitShareDot';
import Codicon from '../components/Codicon';
import { loadJson, saveJson } from '../lib/uiStorage';
import { menuTargets, nextRowSelection, toggleVisibleSelection } from '../lib/policySelection';
import {
  POLICY_BLADE_DISMISS_MS,
  policyDetailOpen,
  policyWorkspaceHostClass,
} from '../lib/policyBladeMotion';

interface Props {
  selectedName: string | null;
  onSelect: (name: string | null) => void;
  onEditFull: (name: string | null, origin?: string, projectId?: number) => void;
  onMatch: () => void;
}

export default function PolicySkillsPage({ selectedName: selectedNameFromRoute, onSelect, onEditFull, onMatch }: Props) {
  const [skills, setSkills] = useState<PolicySkillRow[]>([]);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; row: PolicySkillRow } | null>(null);
  const [selectedName, setSelectedName] = useState<string | null>(selectedNameFromRoute);
  const [openOrigin, setOpenOrigin] = useState<string | undefined>();
  const [openProjectId, setOpenProjectId] = useState<number | undefined>();
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set(selectedNameFromRoute ? [selectedNameFromRoute] : []));
  const [anchorId, setAnchorId] = useState<string | null>(selectedNameFromRoute);
  const [bladeClosing, setBladeClosing] = useState(false);
  const editorRef = useRef<HTMLDivElement | null>(null);
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;

  const [q, setQ] = useState('');
  const [scope, setScope] = useState('');
  const [origin, setOrigin] = useState('');
  const [tags, setTags] = useState<string[]>([]);
  const [sortKey, setSortKey] = useState<SkillSortKey>('name');
  const [sortDir, setSortDir] = useState<SortDir>('asc');
  const [projectStorage, setProjectStorage] = useState<'files' | 'database'>('files');
  const [collapsed, setCollapsed] = useState<Set<string>>(
    () => new Set(loadJson<string[]>('skills-groups-collapsed', [])),
  );
  const [groupIds, setGroupIds] = useState<string[]>(
    () => loadJson<string[]>('skills-groups-filter', []),
  );

  function persistCollapsed(next: Set<string>) {
    saveJson('skills-groups-collapsed', [...next]);
    setCollapsed(next);
  }

  function persistGroupIds(next: string[]) {
    saveJson('skills-groups-filter', next);
    setGroupIds(next);
  }

  function toggleGroup(id: string) {
    persistCollapsed(toggleCollapsed(collapsed, id));
  }

  useEffect(() => {
    setSelectedName(selectedNameFromRoute);
    if (selectedNameFromRoute) {
      setSelectedIds(new Set([selectedNameFromRoute]));
      setAnchorId(selectedNameFromRoute);
    }
  }, [selectedNameFromRoute]);

  function selectSkill(name: string | null) {
    setBladeClosing(false);
    setSelectedName(name);
    onSelectRef.current(name);
    if (name) {
      setSelectedIds(new Set([name]));
      setAnchorId(name);
    } else if (selectedIds.size <= 1) {
      setSelectedIds(new Set());
    }
  }

  function requestCloseBlade() {
    if (!selectedName || bladeClosing) return;
    setBladeClosing(true);
  }

  useEffect(() => {
    if (!bladeClosing) return;
    const t = window.setTimeout(() => {
      setBladeClosing(false);
      setSelectedName(null);
      onSelectRef.current(null);
      setSelectedIds((prev) => (prev.size <= 1 ? new Set() : prev));
    }, POLICY_BLADE_DISMISS_MS);
    return () => window.clearTimeout(t);
  }, [bladeClosing]);

  useEffect(() => {
    Promise.all([fetchPolicySkills(), fetchPolicySyncSettings()])
      .then(([r, s]) => {
        setSkills(r.skills);
        setProjectStorage(s.storage === 'database' ? 'database' : 'files');
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  function reloadSkills() {
    fetchPolicySkills()
      .then((r) => setSkills(r.skills))
      .catch((e: Error) => setError(e.message));
  }

  const listed = useMemo(
    () =>
      skills.map(
        (s) =>
          hydratePolicyListItem(s as unknown as Record<string, unknown>) as unknown as PolicySkillRow,
      ),
    [skills],
  );

  const tagOptions = useMemo(() => collectTags(listed), [listed]);

  const searchVisible = useMemo(
    () => sortSkills(filterSkills(listed, { q, scope, tags, origin }), sortKey, sortDir),
    [listed, q, scope, tags, origin, sortKey, sortDir],
  );

  const groupOptions = useMemo(
    () =>
      visibleSkillGroups(searchVisible).map((g) => ({
        id: g.id,
        label: g.label,
        count: g.skills.length,
      })),
    [searchVisible],
  );

  const visible = useMemo(
    () => searchVisible.filter((s) => matchesGroupFilter(skillResolvedGroup(s), groupIds)),
    [searchVisible, groupIds],
  );

  const grouped = useMemo(() => visibleSkillGroups(visible), [visible]);
  const listedIds = useMemo(() => grouped.map((g) => g.id), [grouped]);
  const visibleRowIds = useMemo(() => visible.map((s) => s.name), [visible]);

  useEffect(() => {
    function onPtr(e: PointerEvent) {
      if (!selectedName) return;
      const t = e.target as Node | null;
      if (!t) return;
      if (editorRef.current?.contains(t)) return;
      if ((t as HTMLElement).closest?.('.policy-context-menu')) return;
      if ((t as HTMLElement).closest?.('.policy-table-row')) return;
      if ((t as HTMLElement).closest?.('.policy-list-resize-handle')) return;
      requestCloseBlade();
    }
    document.addEventListener('pointerdown', onPtr);
    return () => document.removeEventListener('pointerdown', onPtr);
  }, [selectedName, bladeClosing]);

  function onSkillRowClick(e: MouseEvent, s: PolicySkillRow) {
    const next = nextRowSelection({
      id: s.name,
      visibleIds: visibleRowIds,
      selected: selectedIds,
      anchor: anchorId,
      metaKey: e.metaKey || e.ctrlKey,
      shiftKey: e.shiftKey,
    });
    setSelectedIds(next.selected);
    setAnchorId(next.anchor);
    setBladeClosing(false);
    setSelectedName(next.openId);
    onSelectRef.current(next.openId);
    if (next.openId) {
      setOpenOrigin(s.origin);
      setOpenProjectId(s.projectId);
    }
  }

  function onSkillContext(e: MouseEvent, s: PolicySkillRow) {
    e.preventDefault();
    if (!selectedIds.has(s.name)) {
      setSelectedIds(new Set([s.name]));
      setAnchorId(s.name);
    }
    setCtxMenu({ x: e.clientX, y: e.clientY, row: s });
  }

  useEffect(() => {
    if (selectedName && !visible.some((s) => s.name === selectedName)) {
      selectSkill(null);
    }
  }, [selectedName, visible]);

  function toggleFilterTag(tag: string) {
    const key = tag.trim().toLowerCase();
    setTags((prev) =>
      prev.some((t) => t.toLowerCase() === key)
        ? prev.filter((t) => t.toLowerCase() !== key)
        : [...prev, tag],
    );
  }

  function toggleFilterScope(rawScope?: string) {
    const normalized = normalizePolicyScope(rawScope);
    setScope((prev) => (prev === normalized ? '' : normalized));
  }

  usePageContext('Skills', !loading && !error ? `${visible.length}/${listed.length} skills` : undefined);

  function setSort(key: SkillSortKey) {
    const next = toggleSort(sortKey, sortDir, key);
    setSortKey(next.key);
    setSortDir(next.dir);
  }

  async function remove(name: string) {
    if (!confirm(`Delete skill "${name}"?`)) return;
    try {
      await deletePolicySkill(name);
      setSkills((prev) => prev.filter((s) => s.name !== name));
      if (selectedName === name) selectSkill(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to delete skill "${name}"`);
    }
  }

  async function toggleEnabled(name: string, enabled: boolean) {
    try {
      await setPolicySkillEnabled(name, enabled);
      setSkills((prev) => prev.map((s) => (s.name === name ? { ...s, enabled } : s)));
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to update skill "${name}"`);
    }
  }

  async function onPolicyMenu(action: PolicyMenuId) {
    const s = ctxMenu?.row;
    if (!s) return;
    const targets = menuTargets(s, selectedIds, listed, (row) => row.name);
    if (action === 'delete') {
      const ok = isGlobalPolicy(s)
        ? confirm(`Delete ${targets.length} item(s) from global.db?`)
        : confirm(`Delete ${targets.length} skill(s)?`);
      if (!ok) return;
    }
    try {
      for (const row of targets) {
        if (action === 'open' && targets.length === 1) {
          selectSkill(row.name);
          setOpenOrigin(row.origin);
          setOpenProjectId(row.projectId);
        }
        if (action === 'edit' && targets.length === 1) onEditFull(row.name, row.origin, row.projectId);
        if (action === 'enable' && !isGlobalPolicy(row)) await toggleEnabled(row.name, true);
        if (action === 'disable' && !isGlobalPolicy(row)) await toggleEnabled(row.name, false);
        if (action === 'move-global' && !isGlobalPolicy(row)) {
          await relocatePolicyItem('skill', row.name, 'global', row.projectId);
        }
        if (action === 'move-project' && isGlobalPolicy(row)) {
          await relocatePolicyItem('skill', row.name, 'project', row.projectId);
        }
        if (action === 'delete') {
          if (isGlobalPolicy(row)) await deletePolicyCopy('skill', row.name, row.projectId);
          else {
            await deletePolicySkill(row.name);
            setSkills((prev) => prev.filter((x) => x.name !== row.name));
          }
        }
      }
      if (action === 'move-global' || action === 'move-project' || (action === 'delete' && targets.some(isGlobalPolicy))) {
        reloadSkills();
      }
      if (action === 'move-global' || action === 'delete') {
        if (targets.some((t) => t.name === selectedName)) selectSkill(null);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Context menu action failed');
    }
  }

  async function toggleProjectStorage(next: 'files' | 'database') {
    try {
      const s = await savePolicySyncSettings({ storage: next });
      setProjectStorage(s.storage === 'database' ? 'database' : 'files');
      reloadSkills();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to update project storage default');
    }
  }

  async function toggleItemStorage(name: string, currentEffective?: string) {
    const next = currentEffective === 'database' ? 'files' : 'database';
    try {
      const res = await setPolicySkillStorage(name, next);
      setSkills((prev) =>
        prev.map((s) =>
          s.name === name
            ? {
                ...s,
                storage: next,
                effectiveStorage: res.effectiveStorage ?? next,
                storageIsOverride: res.storageIsOverride ?? true,
              }
            : s,
        ),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to update storage for "${name}"`);
    }
  }

  if (loading) {
    return (
      <PageShell>
        <PageHero title="Skills" subtitle="Reusable agent skills loaded on demand." />
        <PageLoading label="Loading skills…" />
      </PageShell>
    );
  }

  return (
    <PageShell>
      <PageHero
        title="Skills"
        subtitle="Task-specific instructions agents can load via ax_skill."
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
            <button type="button" className="btn btn-subtle" onClick={onMatch}>Test match</button>
            <PolicyZipPackageButtons onRestored={() => void reloadSkills()} />
            <button type="button" className="btn primary" onClick={() => onEditFull(null)}>New skill</button>
          </>
        }
      />

      <PageToasts err={error || null} />

      <PageStack>
        <PageCard title="All skills" description="Grouped by catalog folders. Empty groups stay hidden until a skill is assigned.">
          <PolicyToolbar>
            <LabelAutocomplete
              options={tagOptions}
              selected={tags}
              onSelectedChange={setTags}
              query={q}
              onQueryChange={setQ}
              placeholder="Search name or add label…"
              ariaLabel="Filter skills by labels and text"
            />
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
              <PageEmpty title="No skills yet">Create your first skill to guide agent workflows.</PageEmpty>
            ) : visible.length === 0 ? (
              <PageEmpty title="No matching skills">Adjust your search query.</PageEmpty>
            ) : (
              <div className={`page-split policy-rules-split${policyDetailOpen(selectedName, bladeClosing) ? ' page-split--with-detail' : ''}`}>
                <div className="page-split-main">
                  {selectedName ? (
                    <div className="policy-split-id-table">
                    <DataTable dense>
                      <thead>
                        <tr>
                          <th className="policy-col-check">
                            <input
                              type="checkbox"
                              aria-label="Select all visible skills"
                              checked={visibleRowIds.length > 0 && visibleRowIds.every((id) => selectedIds.has(id))}
                              onChange={() => setSelectedIds(toggleVisibleSelection(visibleRowIds, selectedIds))}
                            />
                          </th>
                          <th>Name</th>
                        </tr>
                      </thead>
                      <tbody>
                        {grouped.flatMap((g) => {
                          const open = !collapsed.has(g.id);
                          const header = (
                            <tr key={`group-${g.id}`} className="policy-skill-group-row">
                              <td colSpan={2}>
                                <button
                                  type="button"
                                  className="policy-skill-group-toggle"
                                  aria-expanded={open}
                                  onClick={() => toggleGroup(g.id)}
                                >
                                  <Codicon name={open ? 'chevron-down' : 'chevron-right'} className="policy-skill-group-chevron" />
                                  <span>{g.label}</span>
                                  <span className="muted">{g.skills.length}</span>
                                </button>
                              </td>
                            </tr>
                          );
                          const children = open
                            ? g.skills.map((s) => (
                                <tr
                                  key={s.rowKey ?? s.name}
                                  className={`policy-table-row policy-table-row--nested${s.enabled === false ? ' policy-table-row--disabled' : ''}${selectedIds.has(s.name) ? ' policy-table-row--selected' : ''}`}
                                  style={policyDbRowStyle(s.origin)}
                                  onContextMenu={(e) => onSkillContext(e, s)}
                                  onClick={(e) => onSkillRowClick(e, s)}
                                >
                                  <td className="policy-col-check" onClick={(e) => e.stopPropagation()}>
                                    <input
                                      type="checkbox"
                                      checked={selectedIds.has(s.name)}
                                      aria-label={`Select ${s.name}`}
                                      onChange={(e) => {
                                        const next = new Set(selectedIds);
                                        if (e.target.checked) next.add(s.name);
                                        else next.delete(s.name);
                                        setSelectedIds(next);
                                        setAnchorId(s.name);
                                      }}
                                    />
                                  </td>
                                  <td className="mono">
                                    <span className="policy-id-with-git">
                                      <button type="button" className="policy-link" onClick={(e) => { e.stopPropagation(); onSkillRowClick(e, s); }}>
                                        {s.name}
                                      </button>
                                      <GitShareDot scope={s.scope} enabled={s.enabled} />
                                      <OriginDbBadge origin={s.origin} projectName={s.projectName} />
                                    </span>
                                  </td>
                                </tr>
                              ))
                            : [];
                          return [header, ...children];
                        })}
                      </tbody>
                    </DataTable>
                    </div>
                  ) : (
                    <DataTable dense>
                      <thead>
                        <tr>
                          <th className="policy-col-check">
                            <input
                              type="checkbox"
                              aria-label="Select all visible skills"
                              checked={visibleRowIds.length > 0 && visibleRowIds.every((id) => selectedIds.has(id))}
                              onChange={() => setSelectedIds(toggleVisibleSelection(visibleRowIds, selectedIds))}
                            />
                          </th>
                          <SortTh label="Name" active={sortKey === 'name'} dir={sortDir} onClick={() => setSort('name')} />
                          <th>Database</th>
                          <th>Description</th>
                          <SortTh label="Layer" active={sortKey === 'scope'} dir={sortDir} onClick={() => setSort('scope')} />
                          <th>Tags</th>
                          <th>Enabled</th>
                          <th title="Files (MD) vs Database — override project default">Storage</th>
                          <SortTh label="Pri" active={sortKey === 'priority'} dir={sortDir} onClick={() => setSort('priority')} className="col-num" />
                          <SortTh label="Triggers" active={sortKey === 'triggers'} dir={sortDir} onClick={() => setSort('triggers')} className="col-num" />
                          <th className="col-actions">Actions</th>
                        </tr>
                      </thead>
                      <tbody>
                        {grouped.flatMap((g) => {
                          const open = !collapsed.has(g.id);
                          const header = (
                            <tr key={`group-${g.id}`} className="policy-skill-group-row">
                              <td colSpan={11}>
                                <button
                                  type="button"
                                  className="policy-skill-group-toggle"
                                  aria-expanded={open}
                                  onClick={() => toggleGroup(g.id)}
                                >
                                  <Codicon name={open ? 'chevron-down' : 'chevron-right'} className="policy-skill-group-chevron" />
                                  <span>{g.label}</span>
                                  <span className="muted">{g.skills.length}</span>
                                </button>
                              </td>
                            </tr>
                          );
                          const children = open
                            ? g.skills.map((s) => (
                          <tr
                            key={s.rowKey ?? s.name}
                            className={`policy-table-row policy-table-row--nested${s.enabled === false ? ' policy-table-row--disabled' : ''}${selectedIds.has(s.name) ? ' policy-table-row--selected' : ''}`}
                            style={policyDbRowStyle(s.origin)}
                            onContextMenu={(e) => onSkillContext(e, s)}
                            onClick={(e) => {
                              const t = e.target as HTMLElement;
                              if (t.closest('button, a, input, select, textarea, label')) return;
                              onSkillRowClick(e, s);
                            }}
                          >
                            <td className="policy-col-check" onClick={(e) => e.stopPropagation()}>
                              <input
                                type="checkbox"
                                checked={selectedIds.has(s.name)}
                                aria-label={`Select ${s.name}`}
                                onChange={(e) => {
                                  const next = new Set(selectedIds);
                                  if (e.target.checked) next.add(s.name);
                                  else next.delete(s.name);
                                  setSelectedIds(next);
                                  setAnchorId(s.name);
                                }}
                              />
                            </td>
                            <td className="mono">
                              <span className="policy-id-with-git">
                                <button type="button" className="policy-link" onClick={(e) => { e.stopPropagation(); onSkillRowClick(e, s); }}>
                                  {s.name}
                                </button>
                                <GitShareDot scope={s.scope} enabled={s.enabled} />
                              </span>
                            </td>
                            <td><OriginDbBadge origin={s.origin} projectName={s.projectName} /></td>
                            <td className="policy-table-desc" title={s.description}>
                              {s.description || '—'}
                            </td>
                            <td>
                              <ScopeBadge
                                scope={s.scope}
                                onClick={() => toggleFilterScope(s.scope)}
                                active={scope === normalizePolicyScope(s.scope)}
                              />
                            </td>
                            <td className="policy-table-tags">
                              <TagList items={s.tags ?? []} onTagClick={toggleFilterTag} activeTags={tags} />
                            </td>
                            <td>
                              {isGlobalPolicy(s) ? (
                                <span className="muted">—</span>
                              ) : (
                              <button
                                type="button"
                                className={`settings-toggle${s.enabled !== false ? ' on' : ''}`}
                                onClick={() => void toggleEnabled(s.name, s.enabled === false)}
                                aria-pressed={s.enabled !== false}
                                aria-label={s.enabled !== false ? `Disable ${s.name}` : `Enable ${s.name}`}
                                title={s.enabled !== false ? 'Enabled — click to disable' : 'Disabled — click to enable'}
                              >
                                <span className="settings-toggle-thumb" />
                              </button>
                              )}
                            </td>
                            <td>
                              {isGlobalPolicy(s) ? (
                                <span className="muted">—</span>
                              ) : (
                              <div className="policy-storage-cell">
                                <button
                                  type="button"
                                  role="switch"
                                  aria-checked={(s.effectiveStorage ?? projectStorage) === 'database'}
                                  className={`settings-toggle${(s.effectiveStorage ?? projectStorage) === 'database' ? ' on' : ''}`}
                                  onClick={() => void toggleItemStorage(s.name, s.effectiveStorage ?? projectStorage)}
                                  aria-label={`Storage for ${s.name}`}
                                  title={
                                    (s.effectiveStorage ?? projectStorage) === 'database'
                                      ? 'Database — click for Files (MD)'
                                      : 'Files (MD) — click for Database'
                                  }
                                >
                                  <span className="settings-toggle-thumb" />
                                </button>
                                <span className={`policy-storage-chip${s.storageIsOverride ? ' override' : ''}`}>
                                  {(s.effectiveStorage ?? projectStorage) === 'database' ? 'DB' : 'MD'}
                                  {s.storageIsOverride ? ' · override' : ''}
                                </span>
                              </div>
                              )}
                            </td>
                            <td className="num">{s.priority}</td>
                            <td className="num">{(s.triggers ?? []).length}</td>
                            <td className="col-actions">
                              <PolicyRowActions
                                onEdit={() => onEditFull(s.name, s.origin, s.projectId)}
                                onDelete={() => {
                                  if (isGlobalPolicy(s)) {
                                    if (confirm(`Delete skill "${s.name}" from global.db?`)) {
                                      void deletePolicyCopy('skill', s.name, s.projectId).then(reloadSkills);
                                    }
                                    return;
                                  }
                                  void remove(s.name);
                                }}
                              />
                            </td>
                          </tr>
                              ))
                            : [];
                          return [header, ...children];
                        })}
                      </tbody>
                    </DataTable>
                  )}
                </div>
                {policyDetailOpen(selectedName, bladeClosing) && selectedName ? (
                  <>
                    <PolicyListResizeHandle />
                    <div ref={editorRef} className={policyWorkspaceHostClass(bladeClosing)}>
                      <PolicySkillInlineWorkspace
                        skillName={selectedName}
                        origin={openOrigin}
                        projectId={openProjectId}
                        onClose={() => requestCloseBlade()}
                        onSaved={reloadSkills}
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
