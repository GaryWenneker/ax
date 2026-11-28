import { useEffect, useState } from 'react';
import { fetchPolicySkill, savePolicySkill, type PolicyOriginQuery } from '../policyApi';
import { joinPolicyList, policyEditorCanSave, policyEditorView, type PolicyEditorLoad } from '../lib/policyEditorLoad';
import MarkdownEditor from '../components/MarkdownEditor';
import { LinkedMarkdown, useItemLinks } from '../components/LinkedBody';
import PolicyMetaResizeHandle from '../components/PolicyEditorResize';
import PolicyRevisionHistory from '../components/PolicyRevisionHistory';
import { SkillMetaView } from '../components/PolicyMetaView';
import PolicyPropertiesEditor from '../components/PolicyPropertiesEditor';
import {
  PageCard,
  PageCardBody,
  PageEmpty,
  PageHero,
  PageLoading,
  PageRow,
  PageShell,
  PageStack,
  PageToasts,
} from '../components/ui/PageLayout';
import { usePageContext } from '../context/UiContext';
import { POLICY_SCOPES, type PolicySkillDoc, type SkillFrontmatter } from '../policyTypes';
import { SKILL_PROPERTY_RESERVED } from '../lib/policyProperties';
import { SKILL_GROUPS, resolveSkillGroup } from '../skillGroups';
import SettingsSelect from '../components/ui/SettingsSelect';
import { splitPolicyList } from '../lib/policyList';

interface Props {
  skillName: string | null;
  origin?: string;
  projectId?: number;
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

export default function PolicySkillEditor({ skillName, origin, projectId, onBack }: Props) {
  const isNew = !skillName;
  const originQ: PolicyOriginQuery | undefined = origin === 'global' ? { origin: 'global', projectId } : undefined;
  const [editing, setEditing] = useState(isNew);
  const [fm, setFm] = useState<SkillFrontmatter>(emptyFm());
  const [body, setBody] = useState('');
  const links = useItemLinks('skill', skillName, origin, body);
  const [triggersText, setTriggersText] = useState('');
  const [tagsText, setTagsText] = useState('');
  const [error, setError] = useState('');
  const [load, setLoad] = useState<PolicyEditorLoad>(isNew ? 'ready' : 'loading');
  const [saving, setSaving] = useState(false);

  function applyDoc(doc: PolicySkillDoc) {
    setFm(normalizeSkillFm(doc.frontmatter));
    setBody(doc.body);
    setTriggersText(joinPolicyList(doc.frontmatter.triggers));
    setTagsText(joinPolicyList(doc.frontmatter.tags));
  }

  /** Load the saved skill. `thenView` leaves edit mode once it arrives (first open, Cancel). */
  function reload(thenView: boolean) {
    if (!skillName) return;
    setLoad('loading');
    setError('');
    fetchPolicySkill(skillName, originQ)
      .then((doc) => {
        applyDoc(doc);
        if (thenView) setEditing(false);
        setLoad('ready');
      })
      .catch((e: Error) => {
        setError(e.message);
        setLoad('failed');
      });
  }

  useEffect(() => reload(true), [skillName, origin, projectId]);

  usePageContext(isNew || editing ? 'Skill editor' : 'Skill', skillName ?? 'new skill');

  async function save() {
    setSaving(true);
    setError('');
    try {
      const tags = splitPolicyList(tagsText);
      if (tags.length === 0) {
        setError('At least one tag is required so skills can be filtered in the list.');
        return;
      }
      const frontmatter: SkillFrontmatter = {
        ...fm,
        enabled: fm.enabled !== false,
        triggers: splitPolicyList(triggersText),
        tags,
      };
      await savePolicySkill(skillName, frontmatter, body, originQ);
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
    setEditing(false);
    reload(true);
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
            {!isNew && skillName && origin !== 'global' ? (
              <PolicyRevisionHistory
                kind="skill"
                itemId={skillName}
                onRestored={() => reload(false)}
              />
            ) : null}
            {!isNew && !editing && load === 'ready' && (
              <button type="button" className="btn primary" onClick={() => setEditing(true)}>
                Edit
              </button>
            )}
            {editing && !isNew && (
              <button type="button" className="btn" onClick={cancelEdit}>Cancel</button>
            )}
            {editing && (
              <button type="button" className="btn primary" disabled={!policyEditorCanSave({ load, saving })} onClick={save}>
                {saving ? 'Saving…' : 'Save'}
              </button>
            )}
          </>
        }
      />

      <PageToasts err={error || null} />

      {policyEditorView(load) === 'spinner' ? (
        <PageLoading />
      ) : policyEditorView(load) === 'retry' ? (
        <PageEmpty title="This skill could not be loaded">
          <button type="button" className="btn" onClick={() => reload(false)}>Retry</button>
        </PageEmpty>
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
                        rows={6}
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
                      <SettingsSelect
                        className="settings-select"
                        value={fm.scope || 'project'}
                        onChange={(e) => setFm({ ...fm, scope: e.target.value })}
                      >
                        {POLICY_SCOPES.map((s) => (
                          <option key={s.value} value={s.value}>{s.label}</option>
                        ))}
                      </SettingsSelect>
                    </PageRow>
                    <PageRow title="Group" description="Catalog folder on the Skills list. Empty groups stay available here.">
                      <SettingsSelect
                        className="settings-select"
                        value={fm.group || 'ungrouped'}
                        onChange={(e) => setFm({ ...fm, group: e.target.value })}
                        aria-label="Skill group"
                      >
                        {SKILL_GROUPS.map((g) => (
                          <option key={g.id} value={g.id}>{g.label}</option>
                        ))}
                      </SettingsSelect>
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
                    <PolicyPropertiesEditor
                      key={skillName ?? 'new'}
                      description="Extra keys sent with this skill in preflight. Built-in fields stay in the rows above."
                      properties={fm.properties}
                      reserved={SKILL_PROPERTY_RESERVED}
                      onChange={(properties) => setFm({ ...fm, properties })}
                    />
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
                    properties={fm.properties}
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
              <MarkdownEditor value={body} onChange={setBody} fill links={links} />
            ) : (
              <LinkedMarkdown links={links} value={body} className="page-md-preview" />
            )}
          </PageCard>
        </div>
      )}
    </PageShell>
  );
}
