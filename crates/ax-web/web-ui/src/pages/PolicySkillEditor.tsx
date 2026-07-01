import { useEffect, useState } from 'react';
import { fetchPolicySkill, savePolicySkill } from '../policyApi';
import MarkdownEditor from '../components/MarkdownEditor';
import type { SkillFrontmatter } from '../policyTypes';

interface Props {
  skillName: string | null;
  onBack: () => void;
}

const emptyFm = (): SkillFrontmatter => ({
  name: '',
  description: '',
  triggers: [],
  tags: [],
  priority: 50,
});

function parseCsv(s: string): string[] {
  return s.split(',').map((x) => x.trim()).filter(Boolean);
}

export default function PolicySkillEditor({ skillName, onBack }: Props) {
  const [fm, setFm] = useState<SkillFrontmatter>(emptyFm());
  const [body, setBody] = useState('');
  const [triggersText, setTriggersText] = useState('');
  const [tagsText, setTagsText] = useState('');
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!skillName) return;
    fetchPolicySkill(skillName)
      .then((doc) => {
        setFm(doc.frontmatter);
        setBody(doc.body);
        setTriggersText(doc.frontmatter.triggers.join(', '));
        setTagsText(doc.frontmatter.tags.join(', '));
      })
      .catch((e: Error) => setError(e.message));
  }, [skillName]);

  async function save() {
    setSaving(true);
    setError('');
    try {
      const frontmatter: SkillFrontmatter = {
        ...fm,
        triggers: parseCsv(triggersText),
        tags: parseCsv(tagsText),
      };
      await savePolicySkill(skillName, frontmatter, body);
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
        <h1>{skillName ? `Edit skill: ${skillName}` : 'New skill'}</h1>
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
          <label>Name<input value={fm.name} disabled={!!skillName} onChange={(e) => setFm({ ...fm, name: e.target.value })} /></label>
          <label>Description<textarea rows={3} value={fm.description} onChange={(e) => setFm({ ...fm, description: e.target.value })} /></label>
          <label>Priority<input type="number" value={fm.priority} onChange={(e) => setFm({ ...fm, priority: Number(e.target.value) })} /></label>
          <label>Triggers<input value={triggersText} onChange={(e) => setTriggersText(e.target.value)} /></label>
          <label>Tags<input value={tagsText} onChange={(e) => setTagsText(e.target.value)} /></label>
          <label>Context task (optional)<input value={fm.contextTask ?? ''} onChange={(e) => setFm({ ...fm, contextTask: e.target.value || undefined })} /></label>
        </section>
        <section className="md-panel">
          <MarkdownEditor value={body} onChange={setBody} />
        </section>
      </div>
    </div>
  );
}
