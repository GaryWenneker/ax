import { useEffect, useState } from 'react';
import { fetchPolicyRule, savePolicyRule } from '../policyApi';
import MarkdownEditor from '../components/MarkdownEditor';
import type { RuleFrontmatter } from '../policyTypes';

interface Props {
  ruleId: string | null;
  onBack: () => void;
}

const emptyFm = (): RuleFrontmatter => ({
  id: '',
  level: 'WARNING',
  alwaysApply: false,
  globs: [],
  triggers: [],
  tags: [],
  priority: 50,
});

function parseCsv(s: string): string[] {
  return s.split(',').map((x) => x.trim()).filter(Boolean);
}

export default function PolicyRuleEditor({ ruleId, onBack }: Props) {
  const [fm, setFm] = useState<RuleFrontmatter>(emptyFm());
  const [body, setBody] = useState('');
  const [globsText, setGlobsText] = useState('');
  const [triggersText, setTriggersText] = useState('');
  const [tagsText, setTagsText] = useState('');
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!ruleId) return;
    fetchPolicyRule(ruleId)
      .then((doc) => {
        setFm(doc.frontmatter);
        setBody(doc.body);
        setGlobsText(doc.frontmatter.globs.join(', '));
        setTriggersText(doc.frontmatter.triggers.join(', '));
        setTagsText(doc.frontmatter.tags.join(', '));
      })
      .catch((e: Error) => setError(e.message));
  }, [ruleId]);

  async function save() {
    setSaving(true);
    setError('');
    try {
      const frontmatter: RuleFrontmatter = {
        ...fm,
        globs: parseCsv(globsText),
        triggers: parseCsv(triggersText),
        tags: parseCsv(tagsText),
      };
      await savePolicyRule(ruleId, frontmatter, body);
      onBack();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Save failed');
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="page editor-page">
      <div className="page-header">
        <h1>{ruleId ? `Edit rule: ${ruleId}` : 'New rule'}</h1>
        <div className="page-actions">
          <button type="button" className="btn" onClick={onBack}>Back</button>
          <button type="button" className="btn primary" disabled={saving} onClick={save}>
            {saving ? 'Saving…' : 'Save'}
          </button>
        </div>
      </div>
      {error && <p className="error">{error}</p>}
      <div className="editor-grid">
        <section className="form-panel">
          <label>ID<input value={fm.id} disabled={!!ruleId} onChange={(e) => setFm({ ...fm, id: e.target.value })} /></label>
          <label>Level
            <select value={fm.level} onChange={(e) => setFm({ ...fm, level: e.target.value })}>
              <option>CRITICAL</option>
              <option>WARNING</option>
              <option>INFO</option>
            </select>
          </label>
          <label className="checkbox">
            <input type="checkbox" checked={fm.alwaysApply} onChange={(e) => setFm({ ...fm, alwaysApply: e.target.checked })} />
            Always apply
          </label>
          <label>Priority<input type="number" value={fm.priority} onChange={(e) => setFm({ ...fm, priority: Number(e.target.value) })} /></label>
          <label>Globs (comma-separated)<input value={globsText} onChange={(e) => setGlobsText(e.target.value)} placeholder="**/*.tsx, **/*.css" /></label>
          <label>Triggers<input value={triggersText} onChange={(e) => setTriggersText(e.target.value)} placeholder="mobile, deploy" /></label>
          <label>Tags<input value={tagsText} onChange={(e) => setTagsText(e.target.value)} /></label>
        </section>
        <section className="md-panel">
          <MarkdownEditor value={body} onChange={setBody} />
        </section>
      </div>
    </div>
  );
}
