import { useEffect, useState } from 'react';
import { fetchPolicySkill, savePolicySkill } from '../policyApi';
import MarkdownEditor from '../components/MarkdownEditor';
import {
  PageCard,
  PageCardBody,
  PageHero,
  PageRow,
  PageShell,
  PageStack,
  PageToasts,
} from '../components/ui/PageLayout';
import { usePageContext } from '../context/UiContext';
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

  usePageContext('Skill editor', skillName ?? 'new skill');

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
    <PageShell>
      <PageHero
        title={skillName ? `Edit skill: ${skillName}` : 'New skill'}
        subtitle="Configure triggers and skill instructions."
        actions={
          <>
            <button type="button" className="btn" onClick={onBack}>Back</button>
            <button type="button" className="btn primary" disabled={saving} onClick={save}>
              {saving ? 'Saving…' : 'Save'}
            </button>
          </>
        }
      />

      <PageToasts err={error || null} />

      <div className="page-editor-grid">
        <PageStack>
          <PageCard title="Metadata" description="Frontmatter fields for skill matching.">
            <PageCardBody>
              <PageRow title="Name" description="Unique skill identifier.">
                <input
                  className="settings-input"
                  value={fm.name}
                  disabled={!!skillName}
                  onChange={(e) => setFm({ ...fm, name: e.target.value })}
                />
              </PageRow>
              <PageRow title="Description" description="Short summary shown in skill lists.">
                <textarea
                  className="settings-input"
                  rows={3}
                  value={fm.description}
                  onChange={(e) => setFm({ ...fm, description: e.target.value })}
                  style={{ resize: 'vertical' }}
                />
              </PageRow>
              <PageRow title="Priority" description="Higher priority skills sort first.">
                <input
                  className="settings-input settings-input--narrow"
                  type="number"
                  value={fm.priority}
                  onChange={(e) => setFm({ ...fm, priority: Number(e.target.value) })}
                />
              </PageRow>
              <PageRow title="Triggers" description="Keywords that activate this skill.">
                <input className="settings-input" value={triggersText} onChange={(e) => setTriggersText(e.target.value)} />
              </PageRow>
              <PageRow title="Tags" description="Optional categorization tags.">
                <input className="settings-input" value={tagsText} onChange={(e) => setTagsText(e.target.value)} />
              </PageRow>
              <PageRow title="Context task" description="Optional task hint for skill loading.">
                <input
                  className="settings-input"
                  value={fm.contextTask ?? ''}
                  onChange={(e) => setFm({ ...fm, contextTask: e.target.value || undefined })}
                />
              </PageRow>
            </PageCardBody>
          </PageCard>
        </PageStack>

        <PageCard title="Skill body" description="Markdown instructions loaded via ax_skill.">
          <div style={{ minHeight: 400 }}>
            <MarkdownEditor value={body} onChange={setBody} />
          </div>
        </PageCard>
      </div>
    </PageShell>
  );
}
