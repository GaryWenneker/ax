import { useEffect, useMemo, useRef, useState } from 'react';
import {
  deletePolicySkill,
  fetchPolicySkills,
  fetchPolicySyncSettings,
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
import { TagList } from '../components/PolicyMetaView';
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
import {
  collectTags,
  filterSkills,
  normalizePolicyScope,
  sortSkills,
  toggleSort,
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

interface Props {
  selectedName: string | null;
  onSelect: (name: string | null) => void;
  onEditFull: (name: string | null) => void;
  onMatch: () => void;
}

export default function PolicySkillsPage({ selectedName: selectedNameFromRoute, onSelect, onEditFull, onMatch }: Props) {
  const [skills, setSkills] = useState<PolicySkillRow[]>([]);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);
  const [selectedName, setSelectedName] = useState<string | null>(selectedNameFromRoute);
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;

  const [q, setQ] = useState('');
  const [scope, setScope] = useState('');
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
  }, [selectedNameFromRoute]);

  function selectSkill(name: string | null) {
    setSelectedName(name);
    onSelectRef.current(name);
  }

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

  const tagOptions = useMemo(() => collectTags(skills), [skills]);

  const searchVisible = useMemo(
    () => sortSkills(filterSkills(skills, { q, scope, tags }), sortKey, sortDir),
    [skills, q, scope, tags, sortKey, sortDir],
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

  usePageContext('Skills', !loading && !error ? `${visible.length}/${skills.length} skills` : undefined);

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
            <PolicyGroupListControls
              options={groupOptions}
              selectedIds={groupIds}
              onSelectedIds={persistGroupIds}
              onCollapseAll={() => persistCollapsed(collapseAllGroupIds(listedIds))}
              onExpandAll={() => persistCollapsed(expandAllGroupIds())}
              collapseAllDisabled={allListedCollapsed(listedIds, collapsed)}
              expandAllDisabled={allListedExpanded(listedIds, collapsed)}
            />
            <PolicyCount shown={visible.length} total={skills.length} />
          </PolicyToolbar>

          <PageCardBody>
            {skills.length === 0 ? (
              <PageEmpty title="No skills yet">Create your first skill to guide agent workflows.</PageEmpty>
            ) : visible.length === 0 ? (
              <PageEmpty title="No matching skills">Adjust your search query.</PageEmpty>
            ) : (
              <div className={`page-split policy-rules-split${selectedName ? ' page-split--with-detail' : ''}`}>
                <div className="page-split-main">
                  {selectedName ? (
                    <div className="policy-split-id-table">
                    <DataTable dense>
                      <thead>
                        <tr>
                          <th>Name</th>
                        </tr>
                      </thead>
                      <tbody>
                        {grouped.flatMap((g) => {
                          const open = !collapsed.has(g.id);
                          const header = (
                            <tr key={`group-${g.id}`} className="policy-skill-group-row">
                              <td>
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
                                  key={s.name}
                                  className={`policy-table-row policy-table-row--nested${s.enabled === false ? ' policy-table-row--disabled' : ''}${selectedName === s.name ? ' policy-table-row--selected' : ''}`}
                                  onClick={() => selectSkill(s.name)}
                                >
                                  <td className="mono">
                                    <span className="policy-id-with-git">
                                      <button type="button" className="policy-link" onClick={() => selectSkill(s.name)}>
                                        {s.name}
                                      </button>
                                      <GitShareDot scope={s.scope} enabled={s.enabled} />
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
                          <SortTh label="Name" active={sortKey === 'name'} dir={sortDir} onClick={() => setSort('name')} />
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
                              <td colSpan={9}>
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
                            key={s.name}
                            className={`policy-table-row policy-table-row--nested${s.enabled === false ? ' policy-table-row--disabled' : ''}${selectedName === s.name ? ' policy-table-row--selected' : ''}`}
                            onClick={(e) => {
                              const t = e.target as HTMLElement;
                              if (t.closest('button, a, input, select, textarea, label')) return;
                              selectSkill(s.name);
                            }}
                          >
                            <td className="mono">
                              <span className="policy-id-with-git">
                                <button type="button" className="policy-link" onClick={() => selectSkill(s.name)}>
                                  {s.name}
                                </button>
                                <GitShareDot scope={s.scope} enabled={s.enabled} />
                              </span>
                            </td>
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
                              <TagList items={s.tags} onTagClick={toggleFilterTag} activeTags={tags} />
                            </td>
                            <td>
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
                            </td>
                            <td>
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
                            </td>
                            <td className="num">{s.priority}</td>
                            <td className="num">{s.triggers.length}</td>
                            <td className="col-actions">
                              <PolicyRowActions onEdit={() => onEditFull(s.name)} onDelete={() => remove(s.name)} />
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
                {selectedName && (
                  <>
                    <PolicyListResizeHandle />
                    <PolicySkillInlineWorkspace
                      skillName={selectedName}
                      onClose={() => selectSkill(null)}
                      onSaved={reloadSkills}
                    />
                  </>
                )}
              </div>
            )}
          </PageCardBody>
        </PageCard>
      </PageStack>
    </PageShell>
  );
}
