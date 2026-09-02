import { useEffect, useRef, useState } from 'react';
import { fetchPolicySkill, savePolicySkill } from '../policyApi';
import MarkdownEditor from './MarkdownEditor';
import PolicyMetaResizeHandle from './PolicyEditorResize';
import { PageCard, PageCardBody, PageRow } from './ui/PageLayout';
import { Spinner } from './ui/Spinner';
import { POLICY_SCOPES, type SkillFrontmatter } from '../policyTypes';
import { SKILL_GROUPS, resolveSkillGroup } from '../skillGroups';

interface Props {
  skillName: string;
  onClose: () => void;
  onSaved?: () => void;
}

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

export default function PolicySkillInlineWorkspace({ skillName, onClose, onSaved }: Props) {
  const [fm, setFm] = useState<SkillFrontmatter | null>(null);
  const [body, setBody] = useState('');
  const [triggersText, setTriggersText] = useState('');
  const [tagsText, setTagsText] = useState('');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  /** Reveal content when switching rows; first open uses the blade slide-in only. */
  const [reveal, setReveal] = useState(false);
  const prevSkillNameRef = useRef<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const switching = prevSkillNameRef.current !== null && prevSkillNameRef.current !== skillName;
    prevSkillNameRef.current = skillName;
    setReveal(switching);
    setLoading(true);
    setError('');
    setFm(null);
    setBody('');
    fetchPolicySkill(skillName)
      .then((doc) => {
        if (cancelled) return;
        const normalized = normalizeSkillFm(doc.frontmatter);
        setFm(normalized);
        setBody(doc.body);
        setTriggersText(normalized.triggers.join(', '));
        setTagsText(normalized.tags.join(', '));
      })
      .catch((e: Error) => {
        if (!cancelled) setError(e.message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [skillName]);

  async function save() {
    if (!fm) return;
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
      onSaved?.();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Save failed');
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="policy-inline-workspace" role="complementary" aria-label={skillName}>
      <div className="policy-inline-workspace-toolbar">
        <button type="button" className="policy-rule-detail-back" onClick={onClose}>
          ← Back
        </button>
        <span key={skillName} className={`policy-inline-workspace-title mono${reveal ? ' policy-inline-reveal' : ''}`}>
          {skillName}
        </span>
        <div className="policy-inline-workspace-actions">
          <button type="button" className="btn primary" disabled={saving || loading || !fm} onClick={() => void save()}>
            {saving ? 'Saving…' : 'Save'}
          </button>
        </div>
      </div>
      {error ? <p className="settings-toast settings-toast--err policy-inline-workspace-error">{error}</p> : null}
      {loading ? (
        <div className="policy-rule-detail-loading">
          <Spinner />
          <span className="muted">Loading skill…</span>
        </div>
      ) : fm ? (
        <div key={skillName} className={`policy-inline-workspace-grid${reveal ? ' policy-inline-reveal' : ''}`}>
          <PageCard title="Metadata" description="Matching criteria." className="policy-inline-pane policy-inline-pane--props">
            <PageCardBody>
              <PageRow title="Name">
                <input className="settings-input" value={fm.name} disabled />
              </PageRow>
              <PageRow title="Enabled">
                <button
                  type="button"
                  className={`settings-toggle${fm.enabled !== false ? ' on' : ''}`}
                  onClick={() => setFm({ ...fm, enabled: fm.enabled === false })}
                  aria-pressed={fm.enabled !== false}
                >
                  <span className="settings-toggle-thumb" />
                </button>
              </PageRow>
              <PageRow title="Always apply" description="Inject on every agent turn, including empty prompts.">
                <button
                  type="button"
                  className={`settings-toggle${fm.alwaysApply ? ' on' : ''}`}
                  onClick={() => setFm({ ...fm, alwaysApply: !fm.alwaysApply })}
                  aria-pressed={!!fm.alwaysApply}
                >
                  <span className="settings-toggle-thumb" />
                </button>
              </PageRow>
              <PageRow title="Description">
                <textarea
                  className="settings-input"
                  rows={3}
                  value={fm.description}
                  onChange={(e) => setFm({ ...fm, description: e.target.value })}
                  style={{ resize: 'vertical' }}
                />
              </PageRow>
              <PageRow title="Scope">
                <select
                  className="settings-select"
                  value={fm.scope || 'project'}
                  onChange={(e) => setFm({ ...fm, scope: e.target.value })}
                >
                  {POLICY_SCOPES.map((s) => (
                    <option key={s.value} value={s.value}>
                      {s.label}
                    </option>
                  ))}
                </select>
              </PageRow>
              <PageRow title="Storage" description="Override project default, or leave empty to inherit.">
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
              <PageRow title="Source" description="External path or root:id/skills/…/SKILL.md.">
                <input
                  className="settings-input"
                  value={fm.source ?? ''}
                  onChange={(e) => setFm({ ...fm, source: e.target.value || null })}
                  placeholder="root:team-shared/skills/foo/SKILL.md"
                />
              </PageRow>
              <PageRow title="Root id" description="Write under a configured policy.roots mount.">
                <input
                  className="settings-input"
                  value={fm.rootId ?? ''}
                  onChange={(e) => setFm({ ...fm, rootId: e.target.value || null })}
                  placeholder="team-shared"
                />
              </PageRow>
              <PageRow title="Group" description="Catalog folder on the Skills list.">
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
              <PageRow title="Priority">
                <input
                  className="settings-input settings-input--narrow"
                  type="number"
                  value={fm.priority}
                  onChange={(e) => setFm({ ...fm, priority: Number(e.target.value) })}
                />
              </PageRow>
              <PageRow title="Triggers">
                <input className="settings-input" value={triggersText} onChange={(e) => setTriggersText(e.target.value)} />
              </PageRow>
              <PageRow title="Tags">
                <input className="settings-input" value={tagsText} onChange={(e) => setTagsText(e.target.value)} />
              </PageRow>
              <PageRow title="Context task">
                <input
                  className="settings-input"
                  value={fm.contextTask ?? ''}
                  onChange={(e) => setFm({ ...fm, contextTask: e.target.value || undefined })}
                />
              </PageRow>
            </PageCardBody>
          </PageCard>
          <PolicyMetaResizeHandle />
          <PageCard title="Skill body" description="Markdown source." className="policy-inline-pane policy-inline-pane--editor page-md-panel">
            <MarkdownEditor value={body} onChange={setBody} fill />
          </PageCard>
        </div>
      ) : null}
    </div>
  );
}
