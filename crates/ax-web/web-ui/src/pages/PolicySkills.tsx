import { useEffect, useMemo, useState } from 'react';
import { deletePolicySkill, fetchPolicySkills, setPolicySkillEnabled } from '../policyApi';
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
import {
  PolicyCount,
  PolicyRowActions,
  PolicyToolbar,
  SortTh,
} from '../components/ui/PolicyTable';
import {
  filterSkills,
  sortSkills,
  toggleSort,
  type SkillSortKey,
  type SortDir,
} from '../components/ui/policyListUtils';
import { usePageContext } from '../context/UiContext';
import { POLICY_SCOPES, type PolicySkillRow } from '../policyTypes';

interface Props {
  onEdit: (name: string | null) => void;
  onMatch: () => void;
}

export default function PolicySkillsPage({ onEdit, onMatch }: Props) {
  const [skills, setSkills] = useState<PolicySkillRow[]>([]);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);

  const [q, setQ] = useState('');
  const [scope, setScope] = useState('');
  const [sortKey, setSortKey] = useState<SkillSortKey>('priority');
  const [sortDir, setSortDir] = useState<SortDir>('desc');

  useEffect(() => {
    fetchPolicySkills()
      .then((r) => setSkills(r.skills))
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  const visible = useMemo(
    () => sortSkills(filterSkills(skills, { q, scope }), sortKey, sortDir),
    [skills, q, scope, sortKey, sortDir],
  );

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
            <button type="button" className="btn btn-subtle" onClick={onMatch}>Test match</button>
            <button type="button" className="btn primary" onClick={() => onEdit(null)}>New skill</button>
          </>
        }
      />

      <PageToasts err={error || null} />

      <PageStack>
        <PageCard title="All skills" description="Filter and sort to find skills quickly.">
          <PolicyToolbar>
            <input
              className="settings-input policy-toolbar-search"
              type="search"
              placeholder="Search name, description, tags…"
              value={q}
              onChange={(e) => setQ(e.target.value)}
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
            <PolicyCount shown={visible.length} total={skills.length} />
          </PolicyToolbar>

          <PageCardBody>
            {skills.length === 0 ? (
              <PageEmpty title="No skills yet">Create your first skill to guide agent workflows.</PageEmpty>
            ) : visible.length === 0 ? (
              <PageEmpty title="No matching skills">Adjust your search query.</PageEmpty>
            ) : (
              <DataTable dense>
                <thead>
                  <tr>
                    <SortTh label="Name" active={sortKey === 'name'} dir={sortDir} onClick={() => setSort('name')} />
                    <th>Description</th>
                    <SortTh label="Layer" active={sortKey === 'scope'} dir={sortDir} onClick={() => setSort('scope')} />
                    <th>On</th>
                    <SortTh label="Pri" active={sortKey === 'priority'} dir={sortDir} onClick={() => setSort('priority')} className="col-num" />
                    <SortTh label="Triggers" active={sortKey === 'triggers'} dir={sortDir} onClick={() => setSort('triggers')} className="col-num" />
                    <th className="col-actions">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {visible.map((s) => (
                    <tr key={s.name} className="policy-table-row">
                      <td className="mono">
                        <button type="button" className="policy-link" onClick={() => onEdit(s.name)}>
                          {s.name}
                        </button>
                      </td>
                      <td className="policy-table-desc" title={s.description}>
                        {s.description || '—'}
                      </td>
                      <td><ScopeBadge scope={s.scope} /></td>
                      <td>
                        <input
                          type="checkbox"
                          checked={s.enabled !== false}
                          aria-label={`Enable ${s.name}`}
                          onChange={(e) => void toggleEnabled(s.name, e.target.checked)}
                        />
                      </td>
                      <td className="num">{s.priority}</td>
                      <td className="num">{s.triggers.length}</td>
                      <td className="col-actions">
                        <PolicyRowActions onEdit={() => onEdit(s.name)} onDelete={() => remove(s.name)} />
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
