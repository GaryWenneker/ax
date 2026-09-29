import { useMemo, useState } from 'react';
import ModalShell from './ModalShell';
import { proposeGroups } from '../lib/autoGroup';
import { fetchPolicyRule, fetchPolicySkill, savePolicyRule, savePolicySkill } from '../policyApi';
import type { PolicyRuleRow, PolicySkillRow } from '../policyTypes';
import { SKILL_GROUPS, ruleResolvedGroup, skillGroupLabel, skillResolvedGroup } from '../skillGroups';

type Props =
  | { kind: 'rule'; rows: PolicyRuleRow[]; onClose: () => void; onApplied: () => void }
  | { kind: 'skill'; rows: PolicySkillRow[]; onClose: () => void; onApplied: () => void };

interface Candidate {
  key: string;
  name: string;
  origin?: string;
  projectId?: number;
  proposed: string | null;
}

function rowKey(name: string, origin?: string, projectId?: number): string {
  return `${origin ?? ''}:${projectId ?? ''}:${name}`;
}

function candidates(props: Props): Candidate[] {
  const items =
    props.kind === 'rule'
      ? props.rows.map((r) => ({
          key: rowKey(r.id, r.origin, r.projectId),
          name: r.id,
          origin: r.origin,
          projectId: r.projectId,
          group: ruleResolvedGroup(r),
          text: [r.id.replace(/-/g, ' '), ...(r.tags ?? []), ...(r.triggers ?? []), ...(r.globs ?? []), r.body ?? ''].join(' '),
        }))
      : props.rows.map((s) => ({
          key: rowKey(s.name, s.origin, s.projectId),
          name: s.name,
          origin: s.origin,
          projectId: s.projectId,
          group: skillResolvedGroup(s),
          text: [s.name.replace(/-/g, ' '), s.description ?? '', ...(s.tags ?? []), ...(s.triggers ?? []), s.body ?? ''].join(' '),
        }));
  const byKey = new Map(items.map((i) => [i.key, i]));
  return proposeGroups(items, SKILL_GROUPS).map((p) => {
    const item = byKey.get(p.key)!;
    return { key: p.key, name: item.name, origin: item.origin, projectId: item.projectId, proposed: p.group };
  });
}

/** Preview and apply group suggestions for ungrouped rules or skills. */
export default function AutoGroupModal(props: Props) {
  const { kind, onClose, onApplied } = props;
  const rows = useMemo(() => candidates(props), [props]);
  const [choice, setChoice] = useState<Record<string, string>>(() =>
    Object.fromEntries(rows.filter((r) => r.proposed).map((r) => [r.key, r.proposed!])),
  );
  const [checked, setChecked] = useState<Set<string>>(() => new Set(rows.filter((r) => r.proposed).map((r) => r.key)));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const targets = SKILL_GROUPS.filter((g) => g.id !== 'ungrouped');
  const noun = kind === 'rule' ? 'rules' : 'skills';

  function toggle(key: string) {
    setChecked((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  async function apply() {
    setBusy(true);
    setError(null);
    const failed: string[] = [];
    for (const row of rows) {
      const group = choice[row.key];
      if (!checked.has(row.key) || !group) continue;
      const origin = { origin: row.origin, projectId: row.projectId };
      try {
        if (kind === 'rule') {
          const doc = await fetchPolicyRule(row.name, origin);
          await savePolicyRule(row.name, { ...doc.frontmatter, group }, doc.body, origin);
        } else {
          const doc = await fetchPolicySkill(row.name, origin);
          await savePolicySkill(row.name, { ...doc.frontmatter, group }, doc.body, origin);
        }
      } catch (e) {
        failed.push(`${row.name}: ${e instanceof Error ? e.message : String(e)}`);
      }
    }
    setBusy(false);
    if (failed.length) {
      setError(failed.join('\n'));
      return;
    }
    onApplied();
  }

  const selectedCount = rows.filter((r) => checked.has(r.key) && choice[r.key]).length;

  return (
    <ModalShell
      title="Auto-group"
      subtitle={`${rows.length} ungrouped ${noun}`}
      ariaLabel={`Auto-group ${noun}`}
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn btn-subtle" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button type="button" className="btn primary" onClick={() => void apply()} disabled={busy || selectedCount === 0}>
            {busy ? 'Applying…' : `Apply ${selectedCount}`}
          </button>
        </>
      }
    >
      <p className="project-purge-note">
        Suggestions compare each ungrouped item's text with the members of the existing groups. Nothing changes until you
        apply.
      </p>
      {rows.length === 0 && <p className="project-purge-note">Every {kind} already has a group.</p>}
      {error && <p className="project-purge-error">{error}</p>}
      <ul className="auto-group-list">
        {rows.map((row) => (
          <li key={row.key} className="auto-group-row" data-name={row.name}>
            <label className="auto-group-name">
              <input
                type="checkbox"
                checked={checked.has(row.key)}
                disabled={!choice[row.key]}
                onChange={() => toggle(row.key)}
              />
              <span>{row.name}</span>
            </label>
            <select
              aria-label={`Group for ${row.name}`}
              value={choice[row.key] ?? ''}
              onChange={(e) => {
                const value = e.target.value;
                setChoice((prev) => ({ ...prev, [row.key]: value }));
                setChecked((prev) => {
                  const next = new Set(prev);
                  if (value) next.add(row.key);
                  else next.delete(row.key);
                  return next;
                });
              }}
            >
              <option value="">No match</option>
              {targets.map((g) => (
                <option key={g.id} value={g.id}>
                  {skillGroupLabel(g.id)}
                </option>
              ))}
            </select>
          </li>
        ))}
      </ul>
    </ModalShell>
  );
}
