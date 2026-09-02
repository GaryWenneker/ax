import { useEffect, useState } from 'react';
import { fetchPolicySkill, savePolicySkill } from '../policyApi';
import MarkdownEditor from '../components/MarkdownEditor';
import MarkdownPreview from '../components/MarkdownPreview';
import PolicyMetaResizeHandle from '../components/PolicyEditorResize';
import PolicyRevisionHistory from '../components/PolicyRevisionHistory';
import { SkillMetaView } from '../components/PolicyMetaView';
import {
  PageCard,
  PageCardBody,
  PageHero,
  PageLoading,
  PageRow,
  PageShell,
  PageStack,
  PageToasts,
} from '../components/ui/PageLayout';
import { usePageContext } from '../context/UiContext';
import { POLICY_SCOPES, type SkillFrontmatter } from '../policyTypes';
import { SKILL_GROUPS, resolveSkillGroup } from '../skillGroups';

interface Props {
  skillName: string | null;
  onBack: () => void;
}

const emptyFm = (): SkillFrontmatter => ({
  name: '',
  description: '',
  alwaysApply: false,
  triggers: [],
  tags: [],
  priority: 50,
  enabled: true,
  status: 'approved',
  share: false,
  scope: 'project',
  group: 'ungrouped',
});

function parseCsv(s: string): string[] {
  return s.split(',').map((x) => x.trim()).filter(Boolean);
}

function normalizeSkillFm(fm: SkillFrontmatter): SkillFrontmatter {
  return {
    ...fm,
    alwaysApply: !!fm.alwaysApply,
    enabled: fm.enabled !== false,
    triggers: fm.triggers ?? [],
    tags: fm.tags ?? [],
    scope: fm.scope || 'project',
    status: fm.status || 'approved',
    group: resolveSkillGroup(fm.group, fm.name, fm.tags ?? []),
  };
}

export default function PolicySkillEditor({ skillName, onBack }: Props) {
  const isNew = !skillName;
  const [editing, setEditing] = useState(isNew);
  const [fm, setFm] = useState<SkillFrontmatter>(emptyFm());
  const [body, setBody] = useState('');
  const [triggersText, setTriggersText] = useState('');
  const [tagsText, setTagsText] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(!isNew);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!skillName) return;
    setLoading(true);
    setError('');
    fetchPolicySkill(skillName)
      .then((doc) => {
        setFm(normalizeSkillFm(doc.frontmatter));
        setBody(doc.body);
        setTriggersText(doc.frontmatter.triggers.join(', '));
        setTagsText(doc.frontmatter.tags.join(', '));
        setEditing(false);
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false));
  }, [skillName]);

  usePageContext(isNew || editing ? 'Skill editor' : 'Skill', skillName ?? 'new skill');

  async function save() {
    setSaving(true);
    setError('');
    try {
      const tags = parseCsv(tagsText);
      if (tags.length === 0) {
        setError('At least one tag is required so skills can be filtered in the list.');
        return;
      }
      const frontmatter: SkillFrontmatter = {
        ...fm,
        enabled: fm.enabled !== false,
        triggers: parseCsv(triggersText),
        tags,
      };
      await savePolicySkill(skillName, frontmatter, body);
      onBack();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Save failed');
    } finally {
      setSaving(false);
    }
  }

  function cancelEdit() {
    if (isNew) {
      onBack();
      return;
    }
    setError('');
    setEditing(false);
    if (!skillName) return;
    setLoading(true);
    fetchPolicySkill(skillName)
      .then((doc) => {
        setFm(normalizeSkillFm(doc.frontmatter));
        setBody(doc.body);
        setTriggersText(doc.frontmatter.triggers.join(', '));
        setTagsText(doc.frontmatter.tags.join(', '));
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false));
  }

  const title = isNew ? 'New skill' : editing ? `Edit skill: ${skillName}` : skillName!;
  const subtitle = isNew
    ? 'Configure triggers and skill instructions.'
    : editing
      ? 'Update triggers and skill instructions.'
      : 'Skill metadata and instructions — loaded via ax_skill when triggers match.';

  return (
    <PageShell className="policy-editor-page">
      <PageHero
        title={title}
        subtitle={subtitle}
        actions={
          <>
            <button type="button" className="btn" onClick={onBack}>Back</button>
            {!isNew && skillName ? (
              <PolicyRevisionHistory
                kind="skill"
                itemId={skillName}
                onRestored={() => {
                  setLoading(true);
                  fetchPolicySkill(skillName)
                    .then((doc) => {
                      setFm(normalizeSkillFm(doc.frontmatter));
                      setBody(doc.body);
                      setTriggersText(doc.frontmatter.triggers.join(', '));
                      setTagsText(doc.frontmatter.tags.join(', '));
                    })
                    .catch((e: Error) => setError(e.message))
                    .finally(() => setLoading(false));
                }}
              />
            ) : null}
            {!isNew && !editing && (
              <button type="button" className="btn primary" onClick={() => setEditing(true)}>
                Edit
              </button>
            )}
            {editing && !isNew && (
              <button type="button" className="btn" onClick={cancelEdit}>Cancel</button>
            )}
            {editing && (
              <button type="button" className="btn primary" disabled={saving || loading} onClick={save}>
                {saving ? 'Saving…' : 'Save'}
              </button>
            )}
          </>
        }
      />

      <PageToasts err={error || null} />

      {loading ? (
        <PageLoading />
      ) : (
        <div className="page-editor-grid">
          <PageStack>
            <PageCard
              title="Metadata"
              description={editing ? 'Frontmatter fields for skill matching.' : 'How this skill is matched and prioritized.'}
            >
              <PageCardBody>
                {editing ? (
                  <>
                    <PageRow title="Name" description="Unique skill identifier.">
                      <input
                        className="settings-input"
                        value={fm.name}
                        disabled={!!skillName}
                        onChange={(e) => setFm({ ...fm, name: e.target.value })}
                      />
                    </PageRow>
                    <PageRow
                      title="Enabled"
                      description="Off = skipped by preflight/matcher (keep the skill without deleting it)."
                    >
                      <button
                        type="button"
                        className={`settings-toggle${fm.enabled !== false ? ' on' : ''}`}
                        onClick={() => setFm({ ...fm, enabled: fm.enabled === false })}
                        aria-pressed={fm.enabled !== false}
                        aria-label="Enabled"
                      >
                        <span className="settings-toggle-thumb" />
                      </button>
                    </PageRow>
                    <PageRow
                      title="Always apply"
                      description="Matching: inject this skill on every agent turn, including empty prompts. Not the same as Enabled."
                    >
                      <button
                        type="button"
                        className={`settings-toggle${fm.alwaysApply ? ' on' : ''}`}
                        onClick={() => setFm({ ...fm, alwaysApply: !fm.alwaysApply })}
                        aria-pressed={!!fm.alwaysApply}
                        aria-label="Always apply"
                      >
                        <span className="settings-toggle-thumb" />
                      </button>
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
                    <PageRow title="Priority" description="Higher priority wins when multiple skills match (list is alphabetical by name).">
                      <input
                        className="settings-input settings-input--narrow"
                        type="number"
                        value={fm.priority}
                        onChange={(e) => setFm({ ...fm, priority: Number(e.target.value) })}
                      />
                    </PageRow>
                    <PageRow
                      title="Scope"
                      description="Company/user private live under ~/.ax; project private is gitignored."
                    >
                      <select
                        className="settings-select"
                        value={fm.scope || 'project'}
                        onChange={(e) => setFm({ ...fm, scope: e.target.value })}
                      >
                        {POLICY_SCOPES.map((s) => (
                          <option key={s.value} value={s.value}>{s.label}</option>
                        ))}
                      </select>
                    </PageRow>
                    <PageRow title="Group" description="Catalog folder on the Skills list. Empty groups stay available here.">
                      <select
                        className="settings-select"
                        value={fm.group || 'ungrouped'}
                        onChange={(e) => setFm({ ...fm, group: e.target.value })}
                        aria-label="Skill group"
                      >
                        {SKILL_GROUPS.map((g) => (
                          <option key={g.id} value={g.id}>{g.label}</option>
                        ))}
                      </select>
                    </PageRow>
                    <PageRow title="Triggers" description="Keywords that activate this skill.">
                      <input className="settings-input" value={triggersText} onChange={(e) => setTriggersText(e.target.value)} />
                    </PageRow>
                    <PageRow title="Tags" description="Required — used to filter skills in the list (comma-separated).">
                      <input className="settings-input" value={tagsText} onChange={(e) => setTagsText(e.target.value)} placeholder="e.g. azdo, testing, methodology" />
                    </PageRow>
                    <PageRow title="Context task" description="Optional task hint for skill loading.">
                      <input
                        className="settings-input"
                        value={fm.contextTask ?? ''}
                        onChange={(e) => setFm({ ...fm, contextTask: e.target.value || undefined })}
                      />
                    </PageRow>
                  </>
                ) : (
                  <SkillMetaView
                    name={fm.name}
                    description={fm.description}
                    alwaysApply={!!fm.alwaysApply}
                    priority={fm.priority}
                    triggers={fm.triggers}
                    tags={fm.tags}
                    contextTask={fm.contextTask}
                    scope={fm.scope}
                    enabled={fm.enabled}
                    group={fm.group}
                  />
                )}
              </PageCardBody>
            </PageCard>
          </PageStack>

          <PolicyMetaResizeHandle />

          <PageCard
            title="Skill body"
            description={editing ? 'Markdown instructions loaded via ax_skill.' : 'Rendered instructions loaded via ax_skill.'}
            className="page-md-panel"
          >
            {editing ? (
              <MarkdownEditor value={body} onChange={setBody} fill />
            ) : (
              <MarkdownPreview value={body} className="page-md-preview" />
            )}
          </PageCard>
        </div>
      )}
    </PageShell>
  );
}
