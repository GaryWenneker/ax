import { useCallback, useEffect, useRef, useState } from 'react';
import { fetchUnresolved, fetchUnresolvedSummary, reconcileUnresolved } from '../api';
import NodeDetailPanel from '../components/NodeDetail';
import Codicon from '../components/Codicon';
import ModalShell from '../components/ModalShell';
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
import type { RouteState } from '../lib/routes';
import type { UnresolvedRow, UnresolvedSummary } from '../types';

const LIMIT = 50;

const KIND_OPTIONS = ['', 'calls', 'imports', 'references', 'function_ref', 'extends', 'implements'];

function parseInitialKind(route: RouteState): string {
  return route.kind ?? '';
}

export default function UnresolvedPage({
  route,
  onRouteChange,
}: {
  route: RouteState;
  onRouteChange: (next: RouteState, replace?: boolean) => void;
}) {
  const [refs, setRefs] = useState<UnresolvedRow[]>([]);
  const [summary, setSummary] = useState<UnresolvedSummary | null>(null);
  const [total, setTotal] = useState(0);
  const [q, setQ] = usePersistedString('unresolved-q', '');
  const [kind, setKind] = usePersistedString('unresolved-kind', parseInitialKind(route));
  const [selectedNodeId, setSelectedNodeId] = useState('');

  useEffect(() => {
    const fromUrl = route.kind ?? '';
    if (fromUrl !== kind) setKind(fromUrl);
  }, [route.kind, kind, setKind]);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [reconciling, setReconciling] = useState(false);
  const [enrichOpen, setEnrichOpen] = useState(false);
  const [enriching, setEnriching] = useState(false);
  const [enrichLimit, setEnrichLimit] = useState(200);
  const [lspServers, setLspServers] = useState<
    Array<{ id: string; available: boolean; command?: string; languages?: string[] }>
  >([]);
  const [lspStatusLoading, setLspStatusLoading] = useState(false);
  const [enrichReport, setEnrichReport] = useState<{
    examined: number;
    resolved: number;
    skippedNoServer: number;
    skippedNoDefinition: number;
    errors: string[];
  } | null>(null);
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

  async function loadLspStatus() {
    setLspStatusLoading(true);
    try {
      const r = await fetch('/api/lsp/status');
      const d = await r.json();
      setLspServers(Array.isArray(d.servers) ? d.servers : []);
    } catch {
      setLspServers([]);
    } finally {
      setLspStatusLoading(false);
    }
  }

  async function openEnrich() {
    setEnrichOpen(true);
    setEnrichReport(null);
    await loadLspStatus();
  }

  const availableServerCount = lspServers.filter((s) => s.available).length;

  async function runLspEnrich() {
    if (availableServerCount === 0) return;
    setEnriching(true);
    setError(null);
    setOk(null);
    setEnrichReport(null);
    try {
      const r = await fetch('/api/lsp/enrich', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ limit: enrichLimit }),
      });
      const d = await r.json();
      if (!d.ok) throw new Error(d.error || 'LSP enrich failed');
      const report = d.report ?? {};
      const examined = Number(report.examined ?? 0);
      const resolved = Number(report.resolved ?? 0);
      const skippedNoServer = Number(report.skippedNoServer ?? 0);
      const skippedNoDefinition = Number(report.skippedNoDefinition ?? 0);
      const errors = Array.isArray(report.errors)
        ? report.errors.map((e: unknown) => String(e))
        : [];
      setEnrichReport({
        examined,
        resolved,
        skippedNoServer,
        skippedNoDefinition,
        errors,
      });
      setOk(
        `LSP enrich: examined ${examined}, resolved ${resolved}` +
          (skippedNoServer ? `, no server ${skippedNoServer}` : '') +
          (skippedNoDefinition ? `, no def ${skippedNoDefinition}` : ''),
      );
      fetchUnresolvedSummary().then(setSummary).catch(() => {});
      loadFirst(q, kind);
    } catch (e) {
      setError(String(e));
    } finally {
      setEnriching(false);
    }
  }

  return (
    <PageShell>
      <PageHero
        title="Unresolved references"
        subtitle="Symbol links the indexer could not resolve to a target in the graph — method calls, imports, type references, and similar edges awaiting resolution."
        actions={
          <>
            <button
              type="button"
              className="btn"
              disabled={enriching || loading}
              onClick={() => void openEnrich()}
            >
              Enrich with LSP
            </button>
            <button
              type="button"
              className="btn primary"
              disabled={reconciling || loading}
              onClick={runReconcile}
            >
              {reconciling ? 'Reconciling…' : 'Reconcile references'}
            </button>
          </>
        }
      />

      {enrichOpen && (
        <ModalShell
          title="Enrich with LSP"
          subtitle="Resolve unresolved references via local language servers (Exact edges)."
          onClose={() => !enriching && setEnrichOpen(false)}
          footer={
            <>
              <button
                type="button"
                className="btn"
                disabled={enriching}
                onClick={() => {
                  setEnrichOpen(false);
                  setEnrichReport(null);
                }}
              >
                {enrichReport ? 'Close' : 'Cancel'}
              </button>
              <button
                type="button"
                className="btn primary"
                disabled={enriching || availableServerCount === 0 || lspStatusLoading}
                title={
                  availableServerCount === 0
                    ? 'Install a language server on PATH first'
                    : undefined
                }
                onClick={() => void runLspEnrich()}
              >
                {enriching
                  ? 'Enriching…'
                  : `Run enrich (limit ${enrichLimit})`}
              </button>
            </>
          }
        >
          <div className="lsp-enrich-modal">
            <div className="lsp-enrich-toolbar">
              <label className="lsp-enrich-limit">
                Limit
                <input
                  type="number"
                  className="settings-input settings-input--narrow"
                  min={1}
                  max={5000}
                  step={50}
                  value={enrichLimit}
                  disabled={enriching}
                  onChange={(e) =>
                    setEnrichLimit(Math.max(1, Math.min(5000, Number(e.target.value) || 200)))
                  }
                />
              </label>
              <button
                type="button"
                className="btn"
                disabled={enriching || lspStatusLoading}
                onClick={() => void loadLspStatus()}
              >
                {lspStatusLoading ? 'Checking…' : 'Refresh servers'}
              </button>
            </div>

            <p className="settings-help">
              Language servers on PATH. Unavailable ones are listed but skipped at enrich time.
              Successful definitions become edges with confidence <code>exact</code>.
            </p>

            <ul className="lsp-server-list" aria-label="Language servers">
              {lspServers.length === 0 && !lspStatusLoading ? (
                <li className="lsp-server-list__empty">No servers discovered.</li>
              ) : (
                lspServers.map((s) => (
                  <li
                    key={s.id}
                    className={`lsp-server-row${s.available ? ' lsp-server-row--ok' : ' lsp-server-row--miss'}`}
                  >
                    <span className="lsp-server-id mono">{s.id}</span>
                    <span className="lsp-server-cmd mono">{s.command ?? s.id}</span>
                    <span
                      className={`lsp-server-badge lsp-server-badge--${s.available ? 'ok' : 'miss'}`}
                    >
                      {s.available ? 'available' : 'missing'}
                    </span>
                  </li>
                ))
              )}
            </ul>

            {enrichReport && (
              <div className="lsp-enrich-report" role="status">
                <div className="lsp-enrich-report__title">Last run</div>
                <div className="lsp-enrich-report__grid">
                  <span>Examined</span>
                  <strong>{enrichReport.examined}</strong>
                  <span>Resolved</span>
                  <strong>{enrichReport.resolved}</strong>
                  <span>Skipped (no server)</span>
                  <strong>{enrichReport.skippedNoServer}</strong>
                  <span>Skipped (no definition)</span>
                  <strong>{enrichReport.skippedNoDefinition}</strong>
                </div>
                {enrichReport.errors.length > 0 && (
                  <details className="lsp-enrich-errors">
                    <summary>
                      {enrichReport.errors.length} error
                      {enrichReport.errors.length === 1 ? '' : 's'}
                    </summary>
                    <ul>
                      {enrichReport.errors.slice(0, 12).map((err, i) => (
                        <li key={i} className="mono">
                          {err}
                        </li>
                      ))}
                    </ul>
                  </details>
                )}
              </div>
            )}
          </div>
        </ModalShell>
      )}

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
              onChange={(e) => {
                const nextKind = e.target.value;
                setKind(nextKind);
                onRouteChange({ ...route, kind: nextKind || null }, true);
              }}
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
