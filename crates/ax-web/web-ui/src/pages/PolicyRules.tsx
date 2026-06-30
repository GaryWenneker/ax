import { useEffect, useState } from 'react';
import { deletePolicyRule, fetchPolicyRules } from '../policyApi';
import type { PolicyRuleRow } from '../policyTypes';

interface Props {
  onEdit: (id: string | null) => void;
  onMatch: () => void;
}

export default function PolicyRulesPage({ onEdit, onMatch }: Props) {
  const [rules, setRules] = useState<PolicyRuleRow[]>([]);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetchPolicyRules()
      .then((r) => setRules(r.rules))
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  async function remove(id: string) {
    if (!confirm(`Delete rule "${id}"?`)) return;
    await deletePolicyRule(id);
    setRules((prev) => prev.filter((r) => r.id !== id));
  }

  if (loading) return <p className="muted">Loading rules…</p>;
  if (error) return <p className="error">{error}</p>;

  return (
    <div className="page">
      <div className="page-header">
        <h1>Rules</h1>
        <div className="page-actions">
          <button type="button" className="btn" onClick={onMatch}>Test match</button>
          <button type="button" className="btn primary" onClick={() => onEdit(null)}>New rule</button>
        </div>
      </div>
      {rules.length === 0 ? (
        <p className="muted">No rules yet. Create your first rule.</p>
      ) : (
        <ul className="policy-list">
          {rules.map((r) => (
            <li key={r.id} className={`policy-item level-${r.level.toLowerCase()}`}>
              <div className="policy-item-main">
                <strong>{r.id}</strong>
                <span className="badge">{r.level}</span>
                <span className="muted">p{r.priority}</span>
              </div>
              <div className="policy-item-meta muted">
                {r.alwaysApply ? 'always · ' : ''}
                {r.globs.length} globs · {r.triggers.length} triggers
              </div>
              <div className="policy-item-actions">
                <button type="button" className="btn" onClick={() => onEdit(r.id)}>Edit</button>
                <button type="button" className="btn danger" onClick={() => remove(r.id)}>Delete</button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
