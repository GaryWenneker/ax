import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { conversationLinks } from '../memoryConversation';
import { memoryListChanged } from '../memoryRefresh';
import { useLive, useNewKeys } from '../lib/useLive';
import { createPortal } from 'react-dom';
import {
  captureGitMemories,
  createMemory,
  deleteMemory,
  fetchMemories,
  fetchMemoryFileChanges,
  fetchMemoryFileLinks,
  recallMemories,
  setMemoryEnabled,
  syncDocsCatalog,
  type MemoryMatch,
  type MemoryRow,
} from '../api';
import Codicon from '../components/Codicon';
import ImageLightbox from '../components/ImageLightbox';
import { MemoryDiffHover } from '../components/MemoryDiffHover';
import MarkdownPreview from '../components/MarkdownPreview';
import ModalShell from '../components/ModalShell';
import { Backlinks, interceptLinkClick, useItemLinks } from '../components/LinkedBody';
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
import {
  commitsForMemory,
  fileChangeClass,
  gitFileLinkProps,
  memoryImageCount,
  memoryImageUrl,
  splitMemoryBody,
  type FileChange,
} from '../memoryDetail';
import { memoryRowAge, memoryRowCounts, memoryRowHeadline } from '../memoryHeadline';
import { wikiLinksToMarkdown, type ItemLinks } from '../wikilinks';
import { MEMORY_CATEGORIES, memoryCategory } from '../memoryCategory';
import { usePageContext } from '../context/UiContext';
import { formatInstantInZone, browserTimeZone } from '../lib/timeZone';

const KIND_ICONS: Record<string, string> = {
  decision: 'law',
  bug_fix: 'bug',
  architecture: 'symbol-structure',
  convention: 'checklist',
  note: 'note',
  git: 'git-commit',
};

const KIND_OPTIONS = ['note', 'decision', 'bug_fix', 'architecture', 'convention'];

function fmtMemoryTime(ts: number): string {
  return formatInstantInZone(ts, browserTimeZone()).time.replace(/\.\d{3}$/, '');
}

const ROW_TAG_LIMIT = 3;

/** Tags shown on a row. The git-capture tag duplicates the source pill, so it stays in the detail only. */
function rowTagLabels(source: string | undefined, tags: string[]): { shown: string[]; hidden: string[] } {
  const labels = tags.filter((tag) => !(source === 'git' && tag === 'git'));
  return { shown: labels.slice(0, ROW_TAG_LIMIT), hidden: labels.slice(ROW_TAG_LIMIT) };
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
  const [syncingCatalog, setSyncingCatalog] = useState(false);

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

  const qRef = useRef(q);
  qRef.current = q;
  const refreshRef = useRef<() => void>(() => {});
  useLive('memory', () => refreshRef.current());
  const fresh = useNewKeys(loading ? null : memories.map((m) => m.id));

  useEffect(() => {
    function refreshQuietly() {
      if (document.hidden) return;
      fetchMemories({ limit: 200 })
        .then((page) => {
          setMemories((prev) => (memoryListChanged(prev, page.memories) ? page.memories : prev));
          setTotal(page.total);
        })
        .catch(() => {});
      const query = qRef.current.trim();
      if (!query) return;
      recallMemories(query, 25)
        .then((r) => {
          if (qRef.current.trim() !== query) return;
          setMatches((prev) => (prev && !memoryListChanged(prev, r.matches) ? prev : r.matches));
        })
        .catch(() => {});
    }
    refreshRef.current = refreshQuietly;
    const onVisible = () => { if (!document.hidden) refreshQuietly(); };
    document.addEventListener('visibilitychange', onVisible);
    window.addEventListener('focus', onVisible);
    return () => {
      document.removeEventListener('visibilitychange', onVisible);
      window.removeEventListener('focus', onVisible);
    };
  }, []);

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

  async function runDocsCatalogSync() {
    setSyncingCatalog(true);
    setError(null);
    setMsg(null);
    try {
      const r = await syncDocsCatalog();
      setMsg(
        `Docs catalog: ${r.memoriesBuilt} memories, wiki ${r.wikiAction} (${r.wikiPages} pages), `
        + `${r.importInserted} new / ${r.importUpdated} updated (${Math.round(r.durationMs / 100) / 10}s).`,
      );
      loadAll();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Docs catalog sync failed');
    } finally {
      setSyncingCatalog(false);
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

  async function toggleEnabled(id: string, enabled: boolean) {
    try {
      await setMemoryEnabled(id, enabled);
      const patch = (m: MemoryRow) => (m.id === id ? { ...m, enabled } : m);
      setMemories((prev) => prev.map(patch));
      setMatches((prev) => (prev ? prev.map((m) => ({ ...m, ...patch(m) })) : prev));
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Update failed');
    }
  }

  const shown: Array<MemoryRow & { score?: number }> = matches ?? memories;
  const convLinks = useMemo(() => conversationLinks(shown), [shown]);
  const selected = shown.find((m) => m.id === selectedId) ?? null;

  useEffect(() => {
    if (!selectedId || composerOpen) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') setSelectedId('');
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [selectedId, composerOpen]);

  usePageContext('Memory', `${total} memories${q ? ` · "${q}"` : ''}`);

  const gitCount = memories.filter((m) => m.source === 'git').length;
  const manualCount = memories.filter((m) => m.source !== 'git').length;

  return (
    <PageShell className="memory-page">
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
            <button
              type="button"
              className="btn"
              disabled={syncingCatalog}
              onClick={runDocsCatalogSync}
            >
              {syncingCatalog ? 'Syncing catalog…' : 'Sync docs catalog'}
            </button>
            <InfoHover label="About Sync docs catalog">
              Pulls the wiki set in <code>docsCatalog.wiki_remote</code> (if any), scans <code>.docs/</code>, agent skills,
              and script READMEs, then imports <strong>documentation-catalog</strong> memories into
              ax.db. Same as <code>ax docs-catalog sync</code>.
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

        <div className="mem-overview">
          <div className="mem-how-grid">
            <div className="mem-how-card">
              <div className="mem-how-icon"><Codicon name="git-commit" /></div>
              <div className="mem-how-title">
                Git commits
                <InfoHover label="Git capture details">
                  When you click <strong>Capture from git</strong> or run <code>ax memory capture-git</code>,
                  ax scans recent commits and extracts meaningful ones as memories. Trivial commits
                  (bumps, merges, formatting) are skipped. A <strong>post-commit hook</strong> can
                  capture every non-trivial commit automatically.
                </InfoHover>
              </div>
            </div>
            <div className="mem-how-card">
              <div className="mem-how-icon"><Codicon name="edit" /></div>
              <div className="mem-how-title">
                You or an agent
                <InfoHover label="Manual memory details">
                  Click <strong>New memory</strong> to store a decision, convention, or fix.
                  Agents can also create memories by calling <code>ax_remember</code> via MCP.
                  Duplicate detection warns you if a similar memory already exists.
                </InfoHover>
              </div>
            </div>
            <div className="mem-how-card">
              <div className="mem-how-icon"><Codicon name="rocket" /></div>
              <div className="mem-how-title">
                Every agent turn
                <InfoHover label="Injection details">
                  Every <code>ax_preflight</code> matches the prompt against memories with{' '}
                  <strong>hybrid search</strong> (FTS5 + vector similarity via RRF). The top matches
                  are injected as <code>&lt;ax_memories&gt;</code>. Confidence decays so stale
                  memories rank lower.
                </InfoHover>
              </div>
            </div>
          </div>
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
            <p className="memory-badge-legend">
              Pills are tags saved on that memory — a product, environment, or topic. They have no
              separate definition; the memory text is what they refer to. A row shows the first three.
              Open it to see every tag. <strong>git</strong> means the memory was captured from a commit.
            </p>
            {loading ? (
              <PageLoading />
            ) : shown.length === 0 ? (
              <PageEmpty title={q ? `No memories match "${q}"` : 'No memories yet'}>
                {q
                  ? 'Try different words — recall matches both exact terms and similar phrasing.'
                  : 'Store one with "New memory", run "Capture from git", or commit — post-commit hooks auto-capture non-trivial commits. Agents can also use ax_remember.'}
              </PageEmpty>
            ) : (
              <>
              <div className="memory-kind-legend" aria-label="Node colors by memory kind">
                {MEMORY_CATEGORIES.filter((c) => shown.some((m) => memoryCategory(m.kind) === c.id)).map((c) => (
                  <span key={c.id} className={`memory-kind-legend-item memory-kind-legend-item--${c.id}`}>
                    {c.label}
                  </span>
                ))}
              </div>
              <ItemList className="memory-vault-list">
                    {shown.map((m, i) => {
                      const tags = rowTagLabels(m.source, m.tags);
                      const age = memoryRowAge(m.updated_at, Date.now());
                      const conv = convLinks[i];
                      const images = memoryImageCount(m.body);
                      return (
                      <ItemRow
                        key={m.id}
                        variant="graph"
                        className={`memory-cat--${memoryCategory(m.kind)}${fresh.has(m.id) ? ' live-new' : ''}${
                          conv
                            ? ` memory-conv${conv.joinPrev ? ' memory-conv--prev' : ''}${conv.joinNext ? ' memory-conv--next' : ''}`
                            : ''
                        }`}
                        style={conv ? ({ '--conv-hue': conv.hue } as CSSProperties) : undefined}
                        title={memoryRowHeadline(m)}
                        subtitle={[
                          m.kind,
                          fmtMemoryTime(m.updated_at),
                          memoryRowCounts(m),
                          m.score != null ? `score ${m.score.toFixed(1)}` : '',
                        ]
                          .filter(Boolean)
                          .join(' · ')}
                        meta={age}
                        metaTitle={fmtMemoryTime(m.updated_at)}
                        selected={selectedId === m.id}
                        onClick={() => setSelectedId(m.id === selectedId ? '' : m.id)}
                        badges={
                          <>
                            {m.source === 'git' ? (
                              <span className="page-item-badge" title="Captured from a git commit. This is the source, not a tag.">
                                <Codicon name="git-branch" className="badge-icon" />
                                git
                              </span>
                            ) : (
                              <span className="page-item-badge" title={`Kind: ${m.kind}`}>
                                <Codicon name={KIND_ICONS[m.kind] ?? 'note'} className="badge-icon" />
                                {m.kind.replace(/_/g, ' ')}
                              </span>
                            )}
                            {images > 0 && (
                              <span
                                className="page-item-badge memory-image-badge"
                                title={`Has ${images} image${images === 1 ? '' : 's'}`}
                                aria-label={`Has ${images} image${images === 1 ? '' : 's'}`}
                              >
                                <Codicon name="file-media" className="badge-icon" />
                              </span>
                            )}
                            {conv && (
                              <span
                                className="page-item-badge memory-conv-badge"
                                title={`Turn ${conv.position} of ${conv.count} shown from the same chat (conversation ${conv.key}). Rows with the same colored node belong together.`}
                              >
                                <Codicon name="comment-discussion" className="badge-icon" />
                                {conv.position}/{conv.count}
                              </span>
                            )}
                            {tags.shown.map((t) => (
                              <span key={t} className="page-item-badge" title={`Tag saved on this memory: ${t}. Open the row to see every tag.`}>
                                <Codicon name="tag" className="badge-icon" />
                                {t}
                              </span>
                            ))}
                            {tags.hidden.length > 0 && (
                              <span className="page-item-badge" title={tags.hidden.join(', ')}>
                                <Codicon name="git-branch" className="badge-icon" />
                                +{tags.hidden.length}
                              </span>
                            )}
                          </>
                        }
                        aside={
                          <button
                            type="button"
                            className={`settings-toggle${m.enabled !== false ? ' on' : ''}`}
                            onClick={(e) => {
                              e.stopPropagation();
                              void toggleEnabled(m.id, m.enabled === false);
                            }}
                            aria-pressed={m.enabled !== false}
                            aria-label={m.enabled !== false ? `Disable ${m.title}` : `Enable ${m.title}`}
                            title={m.enabled !== false ? 'Enabled — click to disable' : 'Disabled — click to enable'}
                          >
                            <span className="settings-toggle-thumb" />
                          </button>
                        }
                      />
                      );
                    })}
              </ItemList>
              </>
            )}
          </PageCardBody>
        </PageCard>
      </PageStack>
      {selected && (
        <MemoryBlade
          memory={selected}
          onClose={() => setSelectedId('')}
          onDelete={() => remove(selected.id)}
        />
      )}
    </PageShell>
  );
}

function MemoryMarkdown({
  memoryId,
  value,
  links,
  onImage,
}: {
  memoryId: string;
  value: string;
  links: ItemLinks | null;
  onImage: (src: string, alt: string) => void;
}) {
  const source = links ? wikiLinksToMarkdown(value, links.outgoing) : value;
  return (
    <MarkdownPreview
      value={source}
      className="memory-md"
      openLinksInNewTab
      imageSrc={(src) => memoryImageUrl(memoryId, src)}
      onImageClick={onImage}
      onClick={interceptLinkClick}
    />
  );
}

function FileRow({ path, href }: { path: string; href?: string }) {
  return (
    <>
      <Codicon name="file" className="edge-item-icon" />
      <span className="edge-name">{path}</span>
      {href && (
        <a
          className="memory-file-link"
          href={href}
          target="_blank"
          rel="noopener noreferrer"
          title="Open this file in git"
          aria-label={`Open ${path} in git`}
          onClick={(e) => e.stopPropagation()}
          onPointerDown={(e) => e.stopPropagation()}
        >
          <Codicon name="link-external" />
        </a>
      )}
    </>
  );
}

function MemoryBlade({
  memory,
  onClose,
  onDelete,
}: {
  memory: MemoryRow & { score?: number };
  onClose: () => void;
  onDelete: () => void;
}) {
  const [fileChanges, setFileChanges] = useState<Record<string, FileChange>>({});
  const [fileLinks, setFileLinks] = useState<Record<string, string>>({});
  const [zoomImage, setZoomImage] = useState<{ src: string; alt: string } | null>(null);
  const closeZoomImage = useCallback(() => setZoomImage(null), []);
  useEffect(() => {
    let cancelled = false;
    setFileChanges({});
    setFileLinks({});
    fetchMemoryFileChanges(memory.id)
      .then((result) => {
        if (!cancelled) setFileChanges(result.files ?? {});
      })
      .catch(() => {
        if (!cancelled) setFileChanges({});
      });
    fetchMemoryFileLinks(memory.id)
      .then((result) => {
        if (!cancelled) setFileLinks(result.files ?? {});
      })
      .catch(() => {
        if (!cancelled) setFileLinks({});
      });
    return () => {
      cancelled = true;
    };
  }, [memory.id]);
  const links = useItemLinks('memory', memory.id, undefined, memory.body);
  const host = document.querySelector('.workspace');
  if (!host) return null;
  const headline = memoryRowHeadline(memory);
  const parts = splitMemoryBody(memory.body);
  const filePaths = memory.files.length > 0
    ? memory.files
    : (parts.filesNote ? parts.filesNote.split(',').map((f) => f.trim()).filter(Boolean) : []);
  const commits = commitsForMemory(memory.id, headline, parts.commits);
  return createPortal(
    <aside className="memory-blade" role="complementary" aria-label={headline}>
      {zoomImage && <ImageLightbox src={zoomImage.src} alt={zoomImage.alt} onClose={closeZoomImage} />}
      <div className="detail-header">
        <span className="detail-title memory-blade-title">
          <Codicon name={KIND_ICONS[memory.kind] ?? 'note'} className="detail-title-icon" />
          <span>{headline}</span>
        </span>
        <button type="button" className="detail-close" onClick={onClose} aria-label="Close">
          <Codicon name="close" />
        </button>
      </div>
      <div className="detail-body memory-blade-body">
        <div className="detail-meta">
          <div className="detail-kv">
            <span className="detail-key">Id</span>
            <span className="detail-val">{memory.id}</span>
          </div>
          <div className="detail-kv">
            <span className="detail-key">Kind</span>
            <span className="detail-val">
              {memory.kind}
              <InfoHover label="About memory kinds">
                Kinds help categorize memories for better recall. <strong>note</strong> = general
                knowledge, <strong>decision</strong> = an architectural or process choice,{' '}
                <strong>bug_fix</strong> = root cause and fix, <strong>architecture</strong> = system
                design, <strong>convention</strong> = coding standard or pattern. <strong>turn</strong> =
                captured from an agent turn.
              </InfoHover>
            </span>
          </div>
          <div className="detail-kv">
            <span className="detail-key">Source</span>
            <span className="detail-val">
              {memory.source}
              <InfoHover label="About memory source">
                <strong>git</strong> = auto-captured from a git commit. <strong>user</strong> = manually
                created via the UI or <code>ax memory add</code>. <strong>agent</strong> or{' '}
                <strong>mcp</strong> = stored by an AI agent via <code>ax_remember</code>.{' '}
                <strong>turn-hook</strong> = written when a turn was captured.
              </InfoHover>
            </span>
          </div>
          <div className="detail-kv">
            <span className="detail-key">Enabled</span>
            <span className="detail-val">{memory.enabled !== false ? 'Yes' : 'No'}</span>
          </div>
          <div className="detail-kv">
            <span className="detail-key">Created</span>
            <span className="detail-val">{fmtMemoryTime(memory.created_at)}</span>
          </div>
          <div className="detail-kv">
            <span className="detail-key">Updated</span>
            <span className="detail-val">{fmtMemoryTime(memory.updated_at)}</span>
          </div>
          <div className="detail-kv">
            <span className="detail-key">Confidence</span>
            <span className="detail-val">
              {Math.round(memory.confidence * 100)}%
              <InfoHover label="About confidence">
                Confidence starts at 100% and <strong>decays over time</strong>. Newer memories
                rank higher in recall. This prevents stale knowledge from dominating agent context.
              </InfoHover>
            </span>
          </div>
          {memory.score != null && (
            <div className="detail-kv">
              <span className="detail-key">Score</span>
              <span className="detail-val">
                {memory.score.toFixed(2)}
                <InfoHover label="About recall score">
                  Combined relevance from the same hybrid search agents use during{' '}
                  <code>ax_preflight</code>. Higher means this memory matched the search box more closely.
                </InfoHover>
              </span>
            </div>
          )}
          {parts.request && (
            <div className="detail-kv">
              <span className="detail-key">Request</span>
              <span className="detail-val">{parts.request}</span>
            </div>
          )}
          {parts.conversation && (
            <div className="detail-kv">
              <span className="detail-key">Conversation</span>
              <span className="detail-val">{parts.conversation}</span>
            </div>
          )}
        </div>
        <div>
          <div className="detail-section-title">
            Tags ({memory.tags.length})
            <InfoHover label="About tags">
              Tags are keywords stored with the memory when it was written. Git capture
              stores the tag <strong>git</strong>. An agent or a manual memory can store
              any labels, such as a product or environment name. There is no glossary
              behind a tag — the memory text is the explanation.
            </InfoHover>
          </div>
          {memory.tags.length === 0 ? (
            <p className="muted">No tags on this memory.</p>
          ) : (
            <div className="page-item-badges memory-detail-tags">
              {memory.tags.map((t) => (
                <span key={t} className="page-item-badge" title={`Tag saved on this memory: ${t}`}>
                  <Codicon name="tag" className="badge-icon" />
                  {t}
                </span>
              ))}
            </div>
          )}
        </div>
        <div>
          <div className="detail-section-title">Content</div>
          {parts.prompt ? (
            <MemoryMarkdown
              memoryId={memory.id}
              value={parts.prompt}
              links={links}
              onImage={(src, alt) => setZoomImage({ src, alt })}
            />
          ) : (
            <p className="muted">No content on this memory.</p>
          )}
        </div>
        {parts.outcome && (
          <div>
            <div className="detail-section-title">Outcome</div>
            <MemoryMarkdown
              memoryId={memory.id}
              value={parts.outcome}
              links={links}
              onImage={(src, alt) => setZoomImage({ src, alt })}
            />
          </div>
        )}
        <Backlinks backlinks={links?.backlinks} />
        {filePaths.length > 0 && (
          <div>
            <div className="detail-section-title">
              Files ({filePaths.length})
              <InfoHover label="About associated files">
                Files linked to this memory. For git memories, these are the files touched
                by the commit. For a captured turn, these are the files that changed.
                Green is a file that was added, orange a file that was edited, and red a
                file that was deleted. Click a file or a commit to see its diff, with line
                numbers; Close, Esc, or a click outside it dismisses it. The arrow icon
                next to a file opens it in git. Links in this
                panel open in a new tab. Added lines are green and removed lines are red.
              </InfoHover>
            </div>
            <div className="edge-list">
              {filePaths.map((f) => {
                const change = fileChanges[f];
                const label = change === 'added' ? 'Added' : change === 'modified' ? 'Edited' : change === 'deleted' ? 'Deleted' : undefined;
                const status = label ? `${label} · ${f}` : f;
                return (
                  <MemoryDiffHover
                    key={f}
                    memoryId={memory.id}
                    path={f}
                    heading={status}
                    className={`edge-item edge-item--static memory-file ${fileChangeClass(change)}`}
                    clickToOpen
                  >
                    <FileRow path={f} href={gitFileLinkProps(fileLinks[f])?.href} />
                  </MemoryDiffHover>
                );
              })}
            </div>
          </div>
        )}
        {commits.length > 0 && (
          <div>
            <div className="detail-section-title">Commits</div>
            <div className="edge-list">
              {commits.map((commit) => (
                <MemoryDiffHover
                  key={commit.hash}
                  memoryId={memory.id}
                  commit={commit.hash}
                  heading={`${commit.hash}${commit.subject ? ` ${commit.subject}` : ''}`}
                  className="edge-item edge-item--static memory-commit"
                  clickToOpen
                >
                  <Codicon name="git-commit" className="edge-item-icon" />
                  <span className="edge-name">{commit.hash}</span>
                  {commit.subject && <span className="memory-commit-subject">{commit.subject}</span>}
                </MemoryDiffHover>
              ))}
            </div>
          </div>
        )}
        <div>
          <button type="button" className="btn danger" onClick={onDelete}>
            Delete memory
          </button>
        </div>
      </div>
    </aside>,
    host,
  );
}
