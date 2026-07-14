import { useEffect, useRef, useState } from 'react';
import {
  captureGitMemories,
  createMemory,
  deleteMemory,
  fetchMemories,
  recallMemories,
  type MemoryMatch,
  type MemoryRow,
} from '../api';
import Codicon from '../components/Codicon';
import { ResizableBlade } from '../components/BladeResize';
import ModalShell from '../components/ModalShell';
import { InfoHover } from '../components/ui/InfoHover';
import {
  FilterBar,
  ItemList,
  ItemRow,
  PageCard,
  PageCardBody,
  PageEmpty,
  PageHero,
  PageLoading,
  PageShell,
  PageStack,
  PageToasts,
} from '../components/ui/PageLayout';
import { usePersistedString } from '../hooks/usePersistedState';
import { usePageContext } from '../context/UiContext';

const KIND_ICONS: Record<string, string> = {
  decision: 'law',
  bug_fix: 'bug',
  architecture: 'symbol-structure',
  convention: 'checklist',
  note: 'note',
  git: 'git-commit',
};

const KIND_OPTIONS = ['note', 'decision', 'bug_fix', 'architecture', 'convention'];

function fmtAge(ts: number): string {
  const days = Math.floor((Date.now() - ts) / 86_400_000);
  if (days <= 0) return 'today';
  if (days === 1) return '1 day ago';
  if (days < 30) return `${days} days ago`;
  const months = Math.floor(days / 30);
  return months === 1 ? '1 month ago' : `${months} months ago`;
}

export default function MemoryPage() {
  const [memories, setMemories] = useState<MemoryRow[]>([]);
  const [matches, setMatches] = useState<MemoryMatch[] | null>(null);
  const [total, setTotal] = useState(0);
  const [q, setQ] = usePersistedString('memory-q', '');
  const [selectedId, setSelectedId] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [capturing, setCapturing] = useState(false);

  const [composerOpen, setComposerOpen] = useState(false);
  const [newBody, setNewBody] = useState('');
  const [newTitle, setNewTitle] = useState('');
  const [newKind, setNewKind] = useState('note');
  const [saving, setSaving] = useState(false);

  const debounce = useRef<ReturnType<typeof setTimeout> | null>(null);

  function loadAll() {
    setLoading(true);
    setError(null);
    fetchMemories({ limit: 200 })
      .then((page) => {
        setMemories(page.memories);
        setTotal(page.total);
        setLoading(false);
      })
      .catch((e: Error) => { setError(e.message); setLoading(false); });
  }

  useEffect(() => { loadAll(); }, []);

  useEffect(() => {
    if (debounce.current) clearTimeout(debounce.current);
    if (!q.trim()) { setMatches(null); return; }
    debounce.current = setTimeout(() => {
      recallMemories(q, 25)
        .then((r) => setMatches(r.matches))
        .catch((e: Error) => setError(e.message));
    }, 300);
    return () => { if (debounce.current) clearTimeout(debounce.current); };
  }, [q]);

  async function runCaptureGit() {
    setCapturing(true);
    setError(null);
    setMsg(null);
    try {
      const r = await captureGitMemories(150);
      setMsg(`Git capture: ${r.captured} new, ${r.skipped_existing} already known, ${r.skipped_trivial} trivial skipped (${r.scanned} scanned).`);
      loadAll();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Capture failed');
    } finally {
      setCapturing(false);
    }
  }

  async function saveNew() {
    if (!newBody.trim()) return;
    setSaving(true);
    setError(null);
    try {
      const r = await createMemory({ title: newTitle, body: newBody, kind: newKind });
      setMsg(
        r.similar.length > 0
          ? `Saved — but ${r.similar.length} similar memor${r.similar.length === 1 ? 'y' : 'ies'} already exist${r.similar.length === 1 ? 's' : ''}. Check for contradictions.`
          : 'Memory saved.',
      );
      setComposerOpen(false);
      setNewBody('');
      setNewTitle('');
      setNewKind('note');
      loadAll();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Save failed');
    } finally {
      setSaving(false);
    }
  }

  async function remove(id: string) {
    if (!confirm('Delete this memory?')) return;
    try {
      await deleteMemory(id);
      setMemories((prev) => prev.filter((m) => m.id !== id));
      setMatches((prev) => (prev ? prev.filter((m) => m.id !== id) : prev));
      if (selectedId === id) setSelectedId('');
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Delete failed');
    }
  }

  const shown: Array<MemoryRow & { score?: number }> = matches ?? memories;
  const selected = shown.find((m) => m.id === selectedId) ?? null;

  usePageContext('Memory', `${total} memories${q ? ` · "${q}"` : ''}`);

  const gitCount = memories.filter((m) => m.source === 'git').length;
  const manualCount = memories.filter((m) => m.source !== 'git').length;

  return (
    <PageShell>
      <PageHero
        title="Memory"
        subtitle={
          <>
            Durable project knowledge: decisions, fixes, conventions. Relevant memories are
            injected into every agent turn via <code>ax_preflight</code>.
            <InfoHover label="How Memory works">
              <strong>Memory is your team's long-term knowledge base.</strong> It stores decisions,
              bug fixes, architecture notes, and conventions so AI agents never forget important
              context — even across sessions. Every time an agent calls <code>ax_preflight</code>,
              ax recalls the most relevant memories for the current prompt and injects them into the
              agent's context as <code>&lt;ax_memories&gt;</code>. This happens{' '}
              <strong>automatically</strong> — the agent does not need to ask for memories.
            </InfoHover>
          </>
        }
        actions={
          <>
            <button type="button" className="btn" disabled={capturing} onClick={runCaptureGit}>
              {capturing ? 'Capturing…' : 'Capture from git'}
            </button>
            <InfoHover label="About Capture from git">
              Scans your git log for non-trivial commits (skips merge commits, version bumps, and
              single-line changes) and creates a memory for each one. This gives agents
              historical context about <strong>why</strong> code changed. Duplicates are
              automatically skipped. Run this periodically or set up a post-commit hook.
            </InfoHover>
            <button type="button" className="btn primary" onClick={() => setComposerOpen(true)}>
              New memory
            </button>
            <InfoHover label="About New memory">
              Manually store a decision, convention, or piece of knowledge. When you save, ax
              checks for <strong>similar existing memories</strong> — if duplicates exist, you
              are warned so you can avoid contradictions. Choose a <strong>kind</strong> (note,
              decision, bug_fix, architecture, convention) to help categorize recall.
            </InfoHover>
          </>
        }
      />

      <PageToasts err={error} ok={msg} />

      <PageStack>
        {composerOpen && (
          <ModalShell
            title="New memory"
            subtitle="What should the team (and its agents) never forget?"
            onClose={() => setComposerOpen(false)}
            footer={
              <>
                <button type="button" className="btn btn-subtle" disabled={saving} onClick={() => setComposerOpen(false)}>
                  Cancel
                </button>
                <button type="button" className="btn primary" disabled={saving || !newBody.trim()} onClick={saveNew}>
                  {saving ? 'Saving…' : 'Save memory'}
                </button>
              </>
            }
          >
            <div className="memory-composer ax-modal-form-stack">
              <div className="memory-composer-row ax-modal-form-row">
                <input
                  className="settings-input settings-input--grow"
                  placeholder="Title (optional — defaults to first line)"
                  value={newTitle}
                  onChange={(e) => setNewTitle(e.target.value)}
                />
                <select
                  className="settings-select"
                  value={newKind}
                  onChange={(e) => setNewKind(e.target.value)}
                  aria-label="Memory kind"
                >
                  {KIND_OPTIONS.map((k) => <option key={k} value={k}>{k}</option>)}
                </select>
              </div>
              <textarea
                className="settings-input memory-composer-body"
                placeholder="The decision, fix, or convention — and why."
                rows={5}
                value={newBody}
                onChange={(e) => setNewBody(e.target.value)}
              />
            </div>
          </ModalShell>
        )}

        {/* How it works explainer */}
        <PageCard
          title="How Memory works"
          description="Three ways memories enter the vault, and how they reach your agents."
          info={
            <InfoHover label="Overview">
              Memory is the bridge between your team's past decisions and your AI agents' current
              context. Without memory, every agent session starts from zero.
            </InfoHover>
          }
        >
          <PageCardBody>
            <div className="mem-how-grid">
              <div className="mem-how-card">
                <div className="mem-how-icon"><Codicon name="git-commit" /></div>
                <div className="mem-how-title">
                  Automatic — git commits
                  <InfoHover label="Git capture details">
                    When you click <strong>Capture from git</strong> or run <code>ax memory capture-git</code>,
                    ax scans recent commits and extracts meaningful ones as memories. Trivial commits
                    (bumps, merges, formatting) are skipped. You can also set up a <strong>post-commit
                    hook</strong> so every non-trivial commit is captured automatically.
                  </InfoHover>
                </div>
                <div className="mem-how-desc">
                  Non-trivial git commits are automatically captured as memories with the files they
                  touched. Click "Capture from git" or set up a post-commit hook for continuous capture.
                </div>
              </div>
              <div className="mem-how-card">
                <div className="mem-how-icon"><Codicon name="edit" /></div>
                <div className="mem-how-title">
                  Manual — you or your agent
                  <InfoHover label="Manual memory details">
                    Click <strong>New memory</strong> to store a decision, convention, or fix.
                    Agents can also create memories by calling <code>ax_remember</code> via MCP —
                    for example, when they discover something important during a coding session.
                    Duplicate detection warns you if a similar memory already exists.
                  </InfoHover>
                </div>
                <div className="mem-how-desc">
                  Click "New memory" above, or let agents store knowledge by calling{' '}
                  <code>ax_remember</code> via MCP. Either way, ax checks for duplicates before saving.
                </div>
              </div>
              <div className="mem-how-card">
                <div className="mem-how-icon"><Codicon name="rocket" /></div>
                <div className="mem-how-title">
                  Injected — every agent turn
                  <InfoHover label="Injection details">
                    Every time an agent calls <code>ax_preflight</code> (which happens at the start
                    of every turn per ax policy), the prompt is matched against all memories using{' '}
                    <strong>hybrid search</strong> (FTS5 full-text + vector similarity via RRF).
                    The top-matching memories are injected as <code>&lt;ax_memories&gt;</code> in the
                    preflight response. The agent sees them automatically — no manual recall needed.
                    Confidence decays over time so stale memories rank lower.
                  </InfoHover>
                </div>
                <div className="mem-how-desc">
                  When an agent calls <code>ax_preflight</code>, relevant memories are automatically
                  matched and injected into context. The agent never has to ask — it just knows.
                </div>
              </div>
            </div>
          </PageCardBody>
        </PageCard>

        {/* Stats strip */}
        <div className="mem-stats-strip">
          <div className="mem-stat">
            <span className="mem-stat-value">{total}</span>
            <span className="mem-stat-label">
              total memories
              <InfoHover label="About total">
                All memories stored in <code>ax.db</code>, including git-captured and manually created ones.
              </InfoHover>
            </span>
          </div>
          <div className="mem-stat">
            <span className="mem-stat-value">{gitCount}</span>
            <span className="mem-stat-label">
              from git
              <InfoHover label="About git memories">
                Memories automatically extracted from git commit history. Each captures the commit
                message, the files touched, and the approximate time — giving agents historical
                context about why code changed.
              </InfoHover>
            </span>
          </div>
          <div className="mem-stat">
            <span className="mem-stat-value">{manualCount}</span>
            <span className="mem-stat-label">
              manual / agent
              <InfoHover label="About manual memories">
                Memories created by you via "New memory" or by agents via <code>ax_remember</code>.
                These are typically higher-value: architecture decisions, conventions, bug root
                causes, or knowledge that is not captured in code or commits.
              </InfoHover>
            </span>
          </div>
          <div className="mem-stat">
            <span className="mem-stat-value">hybrid</span>
            <span className="mem-stat-label">
              search mode
              <InfoHover label="About hybrid search">
                Recall uses <strong>two search methods combined</strong>: FTS5 full-text search
                (exact keyword matching) and vector similarity (semantic meaning via local
                embeddings). Results are merged using <strong>Reciprocal Rank Fusion (RRF)</strong>,
                which gives the best of both worlds: exact term hits and semantically similar
                matches. Confidence decay lowers the rank of stale memories over time.
              </InfoHover>
            </span>
          </div>
        </div>

        {/* Memory vault */}
        <PageCard
          title="Memory vault"
          description={`${total.toLocaleString()} memories in ax.db. Hybrid search: full-text + vector similarity with confidence decay.`}
          info={
            <InfoHover label="About the vault">
              This is the full list of stored memories, newest first. Use the search box to
              <strong> recall</strong> — it runs the same hybrid search that agents use during
              preflight. Click any memory to see its full content, files, and metadata. You can
              delete memories that are no longer relevant.
            </InfoHover>
          }
        >
          <FilterBar>
            <input
              className="settings-input settings-input--grow"
              type="search"
              placeholder="Recall — e.g. why did we switch tokenizers?"
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
            <InfoHover label="About recall search">
              Type a question or keywords. This runs the <strong>same hybrid search</strong> that
              agents use during <code>ax_preflight</code> — FTS5 full-text + vector similarity,
              merged via RRF. The <strong>score</strong> shown per result is the combined relevance.
              Use this to test what an agent would "remember" for a given prompt.
            </InfoHover>
          </FilterBar>

          <PageCardBody>
            {loading ? (
              <PageLoading />
            ) : shown.length === 0 ? (
              <PageEmpty title={q ? `No memories match "${q}"` : 'No memories yet'}>
                {q
                  ? 'Try different words — recall matches both exact terms and similar phrasing.'
                  : 'Store one with "New memory", run "Capture from git", or commit — post-commit hooks auto-capture non-trivial commits. Agents can also use ax_remember.'}
              </PageEmpty>
            ) : (
              <div className={`page-split${selected ? ' page-split--with-detail' : ''}`}>
                <div className="page-split-main">
                  <ItemList>
                    {shown.map((m) => (
                      <ItemRow
                        key={m.id}
                        icon={<Codicon name={KIND_ICONS[m.kind] ?? 'note'} className="page-item-codicon" />}
                        title={m.title}
                        subtitle={`${m.kind} · ${fmtAge(m.updated_at)}${m.score != null ? ` · score ${m.score.toFixed(1)}` : ''}`}
                        selected={selectedId === m.id}
                        onClick={() => setSelectedId(m.id === selectedId ? '' : m.id)}
                        badges={
                          <>
                            {m.source === 'git' && <span className="page-item-badge">git</span>}
                            {m.tags.slice(0, 3).map((t) => (
                              <span key={t} className="page-item-badge">{t}</span>
                            ))}
                          </>
                        }
                      />
                    ))}
                  </ItemList>
                </div>
                {selected && (
                  <ResizableBlade>
                  <div className="detail-panel detail-panel--blade" role="complementary" aria-label={selected.title}>
                    <div className="detail-header">
                      <span className="detail-title">
                        <Codicon name={KIND_ICONS[selected.kind] ?? 'note'} className="detail-title-icon" />
                        {selected.title}
                      </span>
                      <button type="button" className="detail-close" onClick={() => setSelectedId('')} aria-label="Close">
                        <Codicon name="close" />
                      </button>
                    </div>
                    <div className="detail-body">
                      <div className="detail-meta">
                        <div className="detail-kv">
                          <span className="detail-key">Kind</span>
                          <span className="detail-val">
                            {selected.kind}
                            <InfoHover label="About memory kinds">
                              Kinds help categorize memories for better recall. <strong>note</strong> = general
                              knowledge, <strong>decision</strong> = an architectural or process choice,{' '}
                              <strong>bug_fix</strong> = root cause and fix, <strong>architecture</strong> = system
                              design, <strong>convention</strong> = coding standard or pattern.
                            </InfoHover>
                          </span>
                        </div>
                        <div className="detail-kv">
                          <span className="detail-key">Source</span>
                          <span className="detail-val">
                            {selected.source}
                            <InfoHover label="About memory source">
                              <strong>git</strong> = auto-captured from a git commit. <strong>user</strong> = manually
                              created via the UI or <code>ax memory add</code>. <strong>agent</strong> = stored by
                              an AI agent via <code>ax_remember</code> MCP tool during a session.
                            </InfoHover>
                          </span>
                        </div>
                        <div className="detail-kv"><span className="detail-key">Updated</span><span className="detail-val">{fmtAge(selected.updated_at)}</span></div>
                        <div className="detail-kv">
                          <span className="detail-key">Confidence</span>
                          <span className="detail-val">
                            {Math.round(selected.confidence * 100)}%
                            <InfoHover label="About confidence">
                              Confidence starts at 100% and <strong>decays over time</strong>. Newer memories
                              rank higher in recall. This prevents stale, outdated knowledge from dominating
                              agent context. You can refresh confidence by editing or re-saving a memory.
                            </InfoHover>
                          </span>
                        </div>
                      </div>
                      <div>
                        <div className="detail-section-title">Content</div>
                        <pre className="detail-code">{selected.body}</pre>
                      </div>
                      {selected.files.length > 0 && (
                        <div>
                          <div className="detail-section-title">
                            Files ({selected.files.length})
                            <InfoHover label="About associated files">
                              Files linked to this memory. For git memories, these are the files touched
                              by the commit. For manual memories, you can optionally attach file paths.
                              During recall, file overlap boosts relevance — if an agent is working on a
                              file that a memory references, that memory ranks higher.
                            </InfoHover>
                          </div>
                          <div className="edge-list">
                            {selected.files.map((f) => (
                              <div key={f} className="edge-item edge-item--static">
                                <Codicon name="file" className="edge-item-icon" />
                                <span className="edge-name">{f}</span>
                              </div>
                            ))}
                          </div>
                        </div>
                      )}
                      <div>
                        <button type="button" className="btn danger" onClick={() => remove(selected.id)}>
                          Delete memory
                        </button>
                      </div>
                    </div>
                  </div>
                  </ResizableBlade>
                )}
              </div>
            )}
          </PageCardBody>
        </PageCard>
      </PageStack>
    </PageShell>
  );
}
