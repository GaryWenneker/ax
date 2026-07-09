import { useEffect, useState } from 'react';
import { fetchPolicyRule, savePolicyRule } from '../policyApi';
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

  usePageContext('Rule editor', ruleId ? ruleId : 'new rule');

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
    <PageShell>
      <PageHero
        title={ruleId ? `Edit rule: ${ruleId}` : 'New rule'}
        subtitle="Configure matching criteria and rule body."
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
          <PageCard title="Metadata" description="Frontmatter fields for rule matching.">
            <PageCardBody>
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
            </PageCardBody>
          </PageCard>
        </PageStack>

        <PageCard title="Rule body" description="Markdown content injected into agent context.">
          <div style={{ minHeight: 400 }}>
            <MarkdownEditor value={body} onChange={setBody} />
          </div>
        </PageCard>
      </div>
    </PageShell>
  );
}
