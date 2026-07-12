import { useCallback, useEffect, useRef, useState } from 'react';
import { fetchUnresolved, fetchUnresolvedSummary, reconcileUnresolved } from '../api';
import NodeDetailPanel from '../components/NodeDetail';
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
  StatusPanel,
  StatusPill,
} from '../components/ui/PageLayout';
import { usePersistedString } from '../hooks/usePersistedState';
import { usePageContext } from '../context/UiContext';
import type { UnresolvedRow, UnresolvedSummary } from '../types';

const LIMIT = 50;

const KIND_OPTIONS = ['', 'calls', 'imports', 'references', 'function_ref', 'extends', 'implements'];

function parseInitialKind(): string {
  const params = new URLSearchParams(window.location.hash.split('?')[1] ?? '');
  return params.get('kind') ?? '';
}

export default function UnresolvedPage() {
  const [refs, setRefs] = useState<UnresolvedRow[]>([]);
  const [summary, setSummary] = useState<UnresolvedSummary | null>(null);
  const [total, setTotal] = useState(0);
  const [q, setQ] = usePersistedString('unresolved-q', '');
  const [kind, setKind] = usePersistedString('unresolved-kind', parseInitialKind());
  const [selectedNodeId, setSelectedNodeId] = useState('');
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [reconciling, setReconciling] = useState(false);
  const [ok, setOk] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const debounce = useRef<ReturnType<typeof setTimeout> | null>(null);
  const fetchGen = useRef(0);
  const scrollRootRef = useRef<HTMLDivElement>(null);
  const sentinelRef = useRef<HTMLDivElement>(null);
  const loadingMoreRef = useRef(false);

  useEffect(() => {
    fetchUnresolvedSummary().then(setSummary).catch(() => {});
  }, []);

  const loadFirst = useCallback(async (newQ: string, newKind: string) => {
    const gen = ++fetchGen.current;
    loadingMoreRef.current = false;
    setLoading(true);
    setError(null);
    try {
      const page = await fetchUnresolved({
        q: newQ,
        kind: newKind || undefined,
        limit: LIMIT,
        offset: 0,
      });
      if (gen !== fetchGen.current) return;
      setRefs(page.refs);
      setTotal(page.total);
    } catch (e) {
      if (gen !== fetchGen.current) return;
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (gen === fetchGen.current) setLoading(false);
    }
  }, []);

  const loadMore = useCallback(async () => {
    if (loading || loadingMoreRef.current || refs.length >= total) return;
    loadingMoreRef.current = true;
    setLoadingMore(true);
    const gen = fetchGen.current;
    const offset = refs.length;
    try {
      const page = await fetchUnresolved({
        q,
        kind: kind || undefined,
        limit: LIMIT,
        offset,
      });
      if (gen !== fetchGen.current) return;
      setRefs((prev) => [...prev, ...page.refs]);
      setTotal(page.total);
    } catch (e) {
      if (gen !== fetchGen.current) return;
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      loadingMoreRef.current = false;
      if (gen === fetchGen.current) setLoadingMore(false);
    }
  }, [loading, refs.length, total, q, kind]);

  useEffect(() => {
    if (debounce.current) clearTimeout(debounce.current);
    debounce.current = setTimeout(() => loadFirst(q, kind), q ? 300 : 0);
    return () => {
      if (debounce.current) clearTimeout(debounce.current);
    };
  }, [q, kind, loadFirst]);

  const hasMore = refs.length < total;

  useEffect(() => {
    if (!hasMore || loading || loadingMoreRef.current) return;
    const sentinel = sentinelRef.current;
    if (!sentinel) return;

    const rootEl = scrollRootRef.current;
    const root =
      rootEl && rootEl.scrollHeight > rootEl.clientHeight + 8 ? rootEl : null;

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) loadMore();
      },
      { root, rootMargin: '320px', threshold: 0 },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [hasMore, loading, loadMore, refs.length, selectedNodeId]);

  usePageContext('Unresolved', total ? `${total.toLocaleString()} references` : undefined);

  async function runReconcile() {
    setReconciling(true);
    setError(null);
    setOk(null);
    try {
      const result = await reconcileUnresolved();
      const pruned = result.pruned
        ? result.pruned.orphan_from_node
          + result.pruned.stale_file
          + result.pruned.malformed_generic
          + result.pruned.external_calls
        : 0;
      setOk(
        `Reconciled: ${result.resolved ?? 0} resolved, ${pruned} pruned, ${result.remaining ?? 0} remaining`,
      );
      fetchUnresolvedSummary().then(setSummary).catch(() => {});
      loadFirst(q, kind);
    } catch (e) {
      setError(String(e));
    } finally {
      setReconciling(false);
    }
  }

  return (
    <PageShell>
      <PageHero
        title="Unresolved references"
        subtitle="Symbol links the indexer could not resolve to a target in the graph — method calls, imports, type references, and similar edges awaiting resolution."
        actions={
          <button
            type="button"
            className="btn primary"
            disabled={reconciling || loading}
            onClick={runReconcile}
          >
            {reconciling ? 'Reconciling…' : 'Reconcile references'}
          </button>
        }
      />

      <PageToasts ok={ok} err={error} />

      <PageStack>
        {summary && summary.total > 0 && (
          <PageCard title="Breakdown" description="Unresolved references grouped by link type.">
            <StatusPanel title="By kind">
              {summary.by_kind.map((k) => (
                <StatusPill
                  key={k.kind}
                  label={k.kind}
                  value={k.count.toLocaleString()}
                  tone={kind === k.kind ? 'ok' : 'neutral'}
                />
              ))}
            </StatusPanel>
          </PageCard>
        )}

        <PageCard
          title="Unresolved list"
          description={
            total
              ? `${total.toLocaleString()} references could not be linked to a known symbol. Use Reconcile to prune stale entries and re-run resolution, or run \`ax index\` after extractor updates.`
              : 'No unresolved references in the index.'
          }
        >
          <FilterBar>
            <input
              className="settings-input settings-input--grow"
              type="search"
              placeholder="Filter by name or file path…"
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
            <select
              className="settings-select"
              value={kind}
              onChange={(e) => setKind(e.target.value)}
              aria-label="Filter by reference kind"
            >
              <option value="">All kinds</option>
              {KIND_OPTIONS.filter(Boolean).map((k) => (
                <option key={k} value={k}>
                  {k}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="btn primary"
              disabled={reconciling || loading}
              onClick={runReconcile}
            >
              {reconciling ? 'Reconciling…' : 'Reconcile'}
            </button>
          </FilterBar>

          <PageCardBody>
            {loading && refs.length === 0 ? (
              <PageLoading />
            ) : refs.length === 0 ? (
              <PageEmpty title="No unresolved references">
                {summary?.total
                  ? 'Try a different filter.'
                  : 'All symbol references resolved — or run an index first.'}
              </PageEmpty>
            ) : (
              <div className={selectedNodeId ? 'page-split page-split--with-detail' : 'page-split'}>
                <div className="page-split-main" ref={scrollRootRef}>
                  <ItemList>
                    {refs.map((r) => (
                      <ItemRow
                        key={r.id}
                        title={r.reference_name}
                        subtitle={`${r.file_path}:${r.line} · ${r.reference_kind}`}
                        badges={<span className="page-item-badge">{r.language}</span>}
                        icon={<Codicon name="symbol-reference" />}
                        selected={selectedNodeId === r.from_node_id}
                        onClick={() => setSelectedNodeId(r.from_node_id)}
                      />
                    ))}
                  </ItemList>
                  <div ref={sentinelRef} className="page-infinite-sentinel" aria-hidden="true" />
                  <div className="page-infinite-status" aria-live="polite">
                    {loadingMore
                      ? 'Loading more…'
                      : hasMore
                        ? `Showing ${refs.length.toLocaleString()} of ${total.toLocaleString()}`
                        : `${refs.length.toLocaleString()} references`}
                  </div>
                </div>
                {selectedNodeId && (
                  <NodeDetailPanel
                    nodeId={selectedNodeId}
                    onClose={() => setSelectedNodeId('')}
                    onNavigate={setSelectedNodeId}
                    variant="blade"
                  />
                )}
              </div>
            )}
          </PageCardBody>
        </PageCard>
      </PageStack>
    </PageShell>
  );
}
