import { useEffect, useState } from 'react';
import { deletePolicySkill, fetchPolicySkills } from '../policyApi';
import { usePageContext } from '../context/UiContext';
import type { PolicySkillRow } from '../policyTypes';

interface Props {
  onEdit: (name: string | null) => void;
  onMatch: () => void;
}

export default function PolicySkillsPage({ onEdit, onMatch }: Props) {
  const [skills, setSkills] = useState<PolicySkillRow[]>([]);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetchPolicySkills()
      .then((r) => setSkills(r.skills))
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  usePageContext('Skills', !loading && !error ? `${skills.length} skills` : undefined);

  async function remove(name: string) {
    if (!confirm(`Delete skill "${name}"?`)) return;
    await deletePolicySkill(name);
    setSkills((prev) => prev.filter((s) => s.name !== name));
  }

  if (loading) return <p className="muted">Loading skills…</p>;
  if (error) return <p className="error">{error}</p>;

  return (
    <div className="page">
      <div className="page-header">
        <h1>Skills</h1>
        <div className="page-actions">
          <button type="button" className="btn" onClick={onMatch}>Test match</button>
          <button type="button" className="btn primary" onClick={() => onEdit(null)}>New skill</button>
        </div>
      </div>
      {skills.length === 0 ? (
        <p className="muted">No skills yet. Create your first skill.</p>
      ) : (
        <ul className="policy-list">
          {skills.map((s) => (
            <li key={s.name} className="policy-item level-info">
              <div className="policy-item-main">
                <strong>{s.name}</strong>
                <span className="muted">p{s.priority}</span>
              </div>
              <div className="policy-item-meta">{s.description}</div>
              <div className="policy-item-actions">
                <button type="button" className="btn" onClick={() => onEdit(s.name)}>Edit</button>
                <button type="button" className="btn danger" onClick={() => remove(s.name)}>Delete</button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
