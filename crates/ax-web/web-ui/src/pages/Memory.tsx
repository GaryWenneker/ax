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

  return (
    <PageShell>
      <PageHero
        title="Memory"
        subtitle="Durable project knowledge: decisions, fixes, conventions. Relevant memories are injected into every agent turn via ax_preflight."
        actions={
          <>
            <button type="button" className="btn" disabled={capturing} onClick={runCaptureGit}>
              {capturing ? 'Capturing…' : 'Capture from git'}
            </button>
            <button type="button" className="btn primary" onClick={() => setComposerOpen((v) => !v)}>
              {composerOpen ? 'Cancel' : 'New memory'}
            </button>
          </>
        }
      />

      <PageToasts err={error} ok={msg} />

      <PageStack>
        {composerOpen && (
          <PageCard
            title="New memory"
            description="What should the team (and its agents) never forget?"
            footer={
              <button type="button" className="btn primary" disabled={saving || !newBody.trim()} onClick={saveNew}>
                {saving ? 'Saving…' : 'Save memory'}
              </button>
            }
          >
            <PageCardBody>
              <div className="memory-composer">
                <div className="memory-composer-row">
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
            </PageCardBody>
          </PageCard>
        )}

        <PageCard
          title="Memory vault"
          description={`${total.toLocaleString()} memories in ax.db. Hybrid search: full-text + vector similarity with confidence decay.`}
        >
          <FilterBar>
            <input
              className="settings-input settings-input--grow"
              type="search"
              placeholder="Recall — e.g. why did we switch tokenizers?"
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
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
                        <div className="detail-kv"><span className="detail-key">Kind</span><span className="detail-val">{selected.kind}</span></div>
                        <div className="detail-kv"><span className="detail-key">Source</span><span className="detail-val">{selected.source}</span></div>
                        <div className="detail-kv"><span className="detail-key">Updated</span><span className="detail-val">{fmtAge(selected.updated_at)}</span></div>
                        <div className="detail-kv"><span className="detail-key">Confidence</span><span className="detail-val">{Math.round(selected.confidence * 100)}%</span></div>
                      </div>
                      <div>
                        <div className="detail-section-title">Content</div>
                        <pre className="detail-code">{selected.body}</pre>
                      </div>
                      {selected.files.length > 0 && (
                        <div>
                          <div className="detail-section-title">Files ({selected.files.length})</div>
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
                )}
              </div>
            )}
          </PageCardBody>
        </PageCard>
      </PageStack>
    </PageShell>
  );
}
