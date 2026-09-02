import { useEffect, useState } from 'react';
import { fetchPolicyRule, savePolicyRule } from '../policyApi';
import MarkdownEditor from '../components/MarkdownEditor';
import MarkdownPreview from '../components/MarkdownPreview';
import PolicyMetaResizeHandle from '../components/PolicyEditorResize';
import PolicyRevisionHistory from '../components/PolicyRevisionHistory';
import { RuleMetaView } from '../components/PolicyMetaView';
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
import { POLICY_SCOPES, type RuleFrontmatter } from '../policyTypes';
import { SKILL_GROUPS, resolveSkillGroup } from '../skillGroups';

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
  enabled: true,
  status: 'approved',
  share: false,
  scope: 'project',
  group: 'ungrouped',
});

function parseCsv(s: string): string[] {
  return s.split(',').map((x) => x.trim()).filter(Boolean);
}

function normalizeRuleFm(fm: RuleFrontmatter): RuleFrontmatter {
  return {
    ...fm,
    alwaysApply: !!fm.alwaysApply,
    enabled: fm.enabled !== false,
    globs: fm.globs ?? [],
    triggers: fm.triggers ?? [],
    tags: fm.tags ?? [],
    scope: fm.scope || 'project',
    status: fm.status || 'approved',
    group: resolveSkillGroup(fm.group, fm.id, fm.tags ?? []),
  };
}

export default function PolicyRuleEditor({ ruleId, onBack }: Props) {
  const isNew = !ruleId;
  const [editing, setEditing] = useState(true);
  const [fm, setFm] = useState<RuleFrontmatter>(emptyFm());
  const [body, setBody] = useState('');
  const [globsText, setGlobsText] = useState('');
  const [triggersText, setTriggersText] = useState('');
  const [tagsText, setTagsText] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(!isNew);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!ruleId) return;
    setLoading(true);
    setError('');
    fetchPolicyRule(ruleId)
      .then((doc) => {
        setFm(normalizeRuleFm(doc.frontmatter));
        setBody(doc.body);
        setGlobsText(doc.frontmatter.globs.join(', '));
        setTriggersText(doc.frontmatter.triggers.join(', '));
        setTagsText(doc.frontmatter.tags.join(', '));
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false));
  }, [ruleId]);

  usePageContext(isNew ? 'New rule' : `Edit rule: ${ruleId}`, ruleId ?? 'new rule');

  async function save() {
    setSaving(true);
    setError('');
    try {
      const tags = parseCsv(tagsText);
      if (tags.length === 0) {
        setError('At least one tag is required so rules can be filtered in the list.');
        return;
      }
      const globs = parseCsv(globsText);
      const triggers = parseCsv(triggersText);
      if (!fm.alwaysApply && globs.length === 0 && triggers.length === 0) {
        setError('Turn on Always apply, or set at least one glob or trigger — otherwise the rule never matches.');
        return;
      }
      const frontmatter: RuleFrontmatter = {
        ...fm,
        alwaysApply: !!fm.alwaysApply,
        enabled: fm.enabled !== false,
        globs,
        triggers,
        tags,
      };
      await savePolicyRule(ruleId, frontmatter, body);
      onBack();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Save failed');
    } finally {
      setSaving(false);
    }
  }

  function cancelEdit() {
    onBack();
  }

  const title = isNew ? 'New rule' : `Edit rule: ${ruleId}`;
  const subtitle = isNew
    ? 'Configure matching criteria and rule body.'
    : 'Update matching criteria and rule body.';

  return (
    <PageShell className="policy-editor-page">
      <PageHero
        title={title}
        subtitle={subtitle}
        actions={
          <>
            <button type="button" className="btn" onClick={cancelEdit}>Cancel</button>
            {!isNew && ruleId ? (
              <PolicyRevisionHistory
                kind="rule"
                itemId={ruleId}
                onRestored={() => {
                  setLoading(true);
                  fetchPolicyRule(ruleId)
                    .then((doc) => {
                      setFm(normalizeRuleFm(doc.frontmatter));
                      setBody(doc.body);
                      setGlobsText(doc.frontmatter.globs.join(', '));
                      setTriggersText(doc.frontmatter.triggers.join(', '));
                      setTagsText(doc.frontmatter.tags.join(', '));
                    })
                    .catch((e: Error) => setError(e.message))
                    .finally(() => setLoading(false));
                }}
              />
            ) : null}
            <button type="button" className="btn primary" disabled={saving || loading} onClick={save}>
              {saving ? 'Saving…' : 'Save'}
            </button>
          </>
        }
      />

      <PageToasts err={error || null} />

      {loading ? (
        <PageLoading />
      ) : (
        <div className="page-editor-grid">
          <PageStack>
            <button type="button" className="policy-meta-back" onClick={onBack}>
              ← Back
            </button>
            <PageCard
              title="Metadata"
              description={editing ? 'Frontmatter fields for rule matching.' : 'How this rule is matched and prioritized.'}
            >
              <PageCardBody>
                {editing ? (
                  <>
                    <PageRow title="ID" description="Unique rule identifier (kebab-case). You can rename when editing.">
                      <input
                        className="settings-input"
                        value={fm.id}
                        onChange={(e) => setFm({ ...fm, id: e.target.value })}
                        pattern="[a-z0-9-]+"
                        title="Kebab-case: lowercase letters, digits, hyphens"
                      />
                    </PageRow>
                    <PageRow title="Level" description="CRITICAL rules block edits via ax_guard.">
                      <select
                        className="settings-select"
                        value={fm.level}
                        onChange={(e) => setFm({ ...fm, level: e.target.value })}
                      >
                        <option>CRITICAL</option>
                        <option>WARNING</option>
                        <option>INFO</option>
                      </select>
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
                    <PageRow
                      title="Storage"
                      description="Per-item override of the project files/database default. Empty = inherit."
                    >
                      <select
                        className="settings-select"
                        value={fm.storage ?? ''}
                        onChange={(e) =>
                          setFm({
                            ...fm,
                            storage: e.target.value
                              ? (e.target.value as 'files' | 'database')
                              : null,
                          })
                        }
                      >
                        <option value="">Project default</option>
                        <option value="files">Files (MD)</option>
                        <option value="database">Database</option>
                      </select>
                    </PageRow>
                    <PageRow
                      title="Source"
                      description="External body path, or root:id/rules/…. Writes a stub in the main tree."
                    >
                      <input
                        className="settings-input"
                        value={fm.source ?? ''}
                        onChange={(e) => setFm({ ...fm, source: e.target.value || null })}
                        placeholder="root:team-shared/rules/foo.mdc"
                      />
                    </PageRow>
                    <PageRow
                      title="Root id"
                      description="Optional policy.roots mount id for file writes."
                    >
                      <input
                        className="settings-input"
                        value={fm.rootId ?? ''}
                        onChange={(e) => setFm({ ...fm, rootId: e.target.value || null })}
                        placeholder="team-shared"
                      />
                    </PageRow>
                    <PageRow
                      title="Enabled"
                      description="Off = skipped by preflight/matcher (keep the rule without deleting it)."
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
                      description="Matching: inject on every agent turn. Not the same as Enabled — if off, set globs or triggers."
                    >
                      <button
                        type="button"
                        className={`settings-toggle${fm.alwaysApply ? ' on' : ''}`}
                        onClick={() => setFm({ ...fm, alwaysApply: !fm.alwaysApply })}
                        aria-pressed={fm.alwaysApply}
                        aria-label="Always apply"
                      >
                        <span className="settings-toggle-thumb" />
                      </button>
                    </PageRow>
                    <PageRow title="Priority" description="Higher priority wins when multiple rules match (list is alphabetical by ID).">
                      <input
                        className="settings-input settings-input--narrow"
                        type="number"
                        value={fm.priority}
                        onChange={(e) => setFm({ ...fm, priority: Number(e.target.value) })}
                      />
                    </PageRow>
                    <PageRow title="Group" description="Catalog folder on the Rules list. Empty groups stay available here.">
                      <select
                        className="settings-select"
                        value={fm.group || 'ungrouped'}
                        onChange={(e) => setFm({ ...fm, group: e.target.value })}
                        aria-label="Rule group"
                      >
                        {SKILL_GROUPS.map((g) => (
                          <option key={g.id} value={g.id}>{g.label}</option>
                        ))}
                      </select>
                    </PageRow>
                    <PageRow title="Globs" description="Comma-separated file patterns.">
                      <input
                        className="settings-input"
                        value={globsText}
                        onChange={(e) => setGlobsText(e.target.value)}
                        placeholder="**/*.tsx, **/*.css"
                      />
                    </PageRow>
                    <PageRow title="Triggers" description="Keywords that activate this rule.">
                      <input
                        className="settings-input"
                        value={triggersText}
                        onChange={(e) => setTriggersText(e.target.value)}
                        placeholder="mobile, deploy"
                      />
                    </PageRow>
                    <PageRow title="Tags" description="Required — used to filter rules in the list (comma-separated).">
                      <input className="settings-input" value={tagsText} onChange={(e) => setTagsText(e.target.value)} placeholder="e.g. azdo, cicd, quality" />
                    </PageRow>
                  </>
                ) : (
                  <RuleMetaView
                    id={fm.id}
                    level={fm.level}
                    alwaysApply={fm.alwaysApply}
                    priority={fm.priority}
                    globs={fm.globs}
                    triggers={fm.triggers}
                    tags={fm.tags}
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
            title="Rule body"
            description={editing ? 'Markdown content injected into agent context.' : 'Rendered markdown injected into agent context.'}
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
