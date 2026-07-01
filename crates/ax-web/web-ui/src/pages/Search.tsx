import { useEffect, useRef, useState } from 'react';
import { fetchSearch } from '../api';
import { usePageContext } from '../context/UiContext';
import type { SearchResult } from '../types';
import NodeDetailPanel from '../components/NodeDetail';

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
    <>
      <div className="page-header">
        <h1 className="page-title">Search</h1>
      </div>

      <div className="search-bar">
        <input
          className="search-input"
          type="search"
          placeholder="Search symbols, functions, classes…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          autoFocus
        />
      </div>

      {error && <div className="state-msg"><strong>Error</strong>{error}</div>}

      {loading && <div className="loading-row">Searching…</div>}

      {!loading && searched && results.length === 0 && (
        <div className="state-msg">
          <strong>No results for "{q}"</strong>
          Try a different query or prefix.
        </div>
      )}

      {!loading && !searched && !q && (
        <div className="state-msg">
          Type to search across all indexed symbols using full-text search.
        </div>
      )}

      {results.length > 0 && (
        <div className="list">
          {results.map((r) => (
            <div
              key={r.id}
              className={`list-item${selectedId === r.id ? ' selected' : ''}`}
              onClick={() => setSelectedId(r.id)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => { if (e.key === 'Enter') setSelectedId(r.id); }}
            >
              <span className="list-item-icon">{r.kind.slice(0, 4)}</span>
              <div className="list-item-body">
                <div className="list-item-name">{r.name}</div>
                <div className="list-item-sub">
                  {r.snippet ? r.snippet : `${r.file_path}:${r.start_line}`}
                </div>
              </div>
              <span className="list-item-badge">{r.language}</span>
            </div>
          ))}
        </div>
      )}

      {selectedId && (
        <NodeDetailPanel
          nodeId={selectedId}
          onClose={() => setSelectedId(null)}
          onNavigate={(id) => setSelectedId(id)}
        />
      )}
    </>
  );
}
