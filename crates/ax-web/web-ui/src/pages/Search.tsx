import { useEffect, useRef, useState } from 'react';
import { fetchSearch } from '../api';
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
} from '../components/ui/PageLayout';
import { usePersistedString } from '../hooks/usePersistedState';
import { usePageContext } from '../context/UiContext';
import type { SearchResult } from '../types';

const KIND_ICONS: Record<string, string> = {
  function: 'symbol-method',
  method: 'symbol-method',
  class: 'symbol-class',
  struct: 'symbol-structure',
  enum: 'symbol-enum',
  trait: 'symbol-interface',
  interface: 'symbol-interface',
  type: 'symbol-type-parameter',
  const: 'symbol-constant',
  variable: 'symbol-variable',
  module: 'symbol-namespace',
  file: 'file',
};

export default function SearchPage() {
  const [q, setQ] = usePersistedString('search-q', '');
  const [results, setResults] = useState<SearchResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searched, setSearched] = useState(false);
  const [selectedId, setSelectedId] = usePersistedString('search-selected', '');
  const debounce = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (debounce.current) clearTimeout(debounce.current);
    if (!q.trim()) { setResults([]); setSearched(false); return; }

    debounce.current = setTimeout(() => {
      setLoading(true);
      setError(null);
      fetchSearch(q, 40)
        .then((page) => { setResults(page.results); setSearched(true); setLoading(false); })
        .catch((e: Error) => { setError(e.message); setLoading(false); });
    }, 280);

    return () => { if (debounce.current) clearTimeout(debounce.current); };
  }, [q]);

  const searchDetail = q.trim()
    ? searched
      ? `${results.length} results · "${q}"`
      : `searching · "${q}"`
    : undefined;
  usePageContext('Search', searchDetail);

  return (
    <PageShell>
      <PageHero
        title="Search"
        subtitle="Full-text search across all indexed symbols."
      />

      <PageToasts err={error} />

      <PageStack>
        <PageCard
          title="Symbol search"
          description={searched && q ? `${results.length} results for "${q}"` : 'Type to search as you go.'}
        >
          <FilterBar>
            <input
              className="settings-input settings-input--grow"
              type="search"
              placeholder="Search symbols, functions, classes…"
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
          </FilterBar>

          <PageCardBody>
            {loading ? (
              <PageLoading label="Searching…" />
            ) : !searched && !q ? (
              <PageEmpty title="Start typing">
                Search across all indexed symbols using full-text search.
              </PageEmpty>
            ) : searched && results.length === 0 ? (
              <PageEmpty title={`No results for "${q}"`}>
                Try a different query or prefix.
              </PageEmpty>
            ) : (
              <div className={`page-split${selectedId ? ' page-split--with-detail' : ''}`}>
                <div className="page-split-main">
                  <ItemList>
                    {results.map((r) => (
                      <ItemRow
                        key={r.id}
                        icon={<Codicon name={KIND_ICONS[r.kind] ?? 'symbol-misc'} className="page-item-codicon" />}
                        title={r.name}
                        subtitle={r.snippet ? r.snippet : `${r.file_path}:${r.start_line}`}
                        selected={selectedId === r.id}
                        onClick={() => setSelectedId(r.id)}
                        badges={<span className="page-item-badge">{r.language}</span>}
                      />
                    ))}
                  </ItemList>
                </div>
                {selectedId && (
                  <NodeDetailPanel
                    nodeId={selectedId}
                    onClose={() => setSelectedId('')}
                    onNavigate={(id) => setSelectedId(id)}
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
