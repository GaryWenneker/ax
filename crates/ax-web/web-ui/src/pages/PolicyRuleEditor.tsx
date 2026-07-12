import { useEffect, useState } from 'react';
import { fetchPolicyRule, savePolicyRule } from '../policyApi';
import MarkdownEditor from '../components/MarkdownEditor';
import MarkdownPreview from '../components/MarkdownPreview';
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
  const isNew = !ruleId;
  const [editing, setEditing] = useState(isNew);
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
        setFm(doc.frontmatter);
        setBody(doc.body);
        setGlobsText(doc.frontmatter.globs.join(', '));
        setTriggersText(doc.frontmatter.triggers.join(', '));
        setTagsText(doc.frontmatter.tags.join(', '));
        setEditing(false);
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false));
  }, [ruleId]);

  usePageContext(isNew || editing ? 'Rule editor' : 'Rule', ruleId ?? 'new rule');

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

  function cancelEdit() {
    if (isNew) {
      onBack();
      return;
    }
    setError('');
    setEditing(false);
    if (!ruleId) return;
    setLoading(true);
    fetchPolicyRule(ruleId)
      .then((doc) => {
        setFm(doc.frontmatter);
        setBody(doc.body);
        setGlobsText(doc.frontmatter.globs.join(', '));
        setTriggersText(doc.frontmatter.triggers.join(', '));
        setTagsText(doc.frontmatter.tags.join(', '));
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false));
  }

  const title = isNew ? 'New rule' : editing ? `Edit rule: ${ruleId}` : ruleId!;
  const subtitle = isNew
    ? 'Configure matching criteria and rule body.'
    : editing
      ? 'Update matching criteria and rule body.'
      : 'Rule metadata and injected markdown — what agents receive via ax_preflight.';

  return (
    <PageShell>
      <PageHero
        title={title}
        subtitle={subtitle}
        actions={
          <>
            <button type="button" className="btn" onClick={onBack}>Back</button>
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
              description={editing ? 'Frontmatter fields for rule matching.' : 'How this rule is matched and prioritized.'}
            >
              <PageCardBody>
                {editing ? (
                  <>
                    <PageRow title="ID" description="Unique rule identifier.">
                      <input
                        className="settings-input"
                        value={fm.id}
                        disabled={!!ruleId}
                        onChange={(e) => setFm({ ...fm, id: e.target.value })}
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
                    <PageRow title="Always apply" description="Inject on every agent turn.">
                      <button
                        type="button"
                        className={`settings-toggle${fm.alwaysApply ? ' on' : ''}`}
                        onClick={() => setFm({ ...fm, alwaysApply: !fm.alwaysApply })}
                        aria-pressed={fm.alwaysApply}
                      >
                        <span className="settings-toggle-thumb" />
                      </button>
                    </PageRow>
                    <PageRow title="Priority" description="Higher priority rules sort first.">
                      <input
                        className="settings-input settings-input--narrow"
                        type="number"
                        value={fm.priority}
                        onChange={(e) => setFm({ ...fm, priority: Number(e.target.value) })}
                      />
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
                    <PageRow title="Tags" description="Optional categorization tags.">
                      <input className="settings-input" value={tagsText} onChange={(e) => setTagsText(e.target.value)} />
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
                  />
                )}
              </PageCardBody>
            </PageCard>
          </PageStack>

          <PageCard
            title="Rule body"
            description={editing ? 'Markdown content injected into agent context.' : 'Rendered markdown injected into agent context.'}
            className="page-md-panel"
          >
            {editing ? (
              <MarkdownEditor value={body} onChange={setBody} />
            ) : (
              <MarkdownPreview value={body} className="page-md-preview" />
            )}
          </PageCard>
        </div>
      )}
    </PageShell>
  );
}
