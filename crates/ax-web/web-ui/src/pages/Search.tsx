import { useEffect, useRef, useState } from 'react';
import { fetchSearch } from '../api';
import NodeDetailPanel from '../components/NodeDetail';
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
import { usePageContext } from '../context/UiContext';
import type { SearchResult } from '../types';

export default function SearchPage() {
  const [q, setQ] = useState('');
  const [results, setResults] = useState<SearchResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searched, setSearched] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
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
              autoFocus
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
              <ItemList>
                {results.map((r) => (
                  <ItemRow
                    key={r.id}
                    icon={r.kind.slice(0, 4)}
                    title={r.name}
                    subtitle={r.snippet ? r.snippet : `${r.file_path}:${r.start_line}`}
                    selected={selectedId === r.id}
                    onClick={() => setSelectedId(r.id)}
                    badges={<span className="page-item-badge">{r.language}</span>}
                  />
                ))}
              </ItemList>
            )}
          </PageCardBody>
        </PageCard>
      </PageStack>

      {selectedId && (
        <NodeDetailPanel
          nodeId={selectedId}
          onClose={() => setSelectedId(null)}
          onNavigate={(id) => setSelectedId(id)}
        />
      )}
    </PageShell>
  );
}
