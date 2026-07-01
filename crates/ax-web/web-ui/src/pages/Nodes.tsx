import { useEffect, useRef, useState } from 'react';
import { fetchNodes } from '../api';
import { usePageContext } from '../context/UiContext';
import type { NodeRow } from '../types';
import NodeDetailPanel from '../components/NodeDetail';

const LIMIT = 50;

const KIND_OPTIONS = [
  '', 'function', 'method', 'class', 'struct', 'enum', 'trait', 'interface',
  'type', 'const', 'variable', 'module', 'file',
];

export default function NodesPage() {
  const [nodes, setNodes] = useState<NodeRow[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [q, setQ] = useState('');
  const [kind, setKind] = useState('');
  const [lang, setLang] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const debounce = useRef<ReturnType<typeof setTimeout> | null>(null);

  function load(newOffset: number, newQ: string, newKind: string, newLang: string) {
    setLoading(true);
    setError(null);
    fetchNodes({ q: newQ, kind: newKind || undefined, lang: newLang || undefined, limit: LIMIT, offset: newOffset })
      .then((page) => {
        setNodes(page.nodes);
        setTotal(page.total);
        setLoading(false);
      })
      .catch((e: Error) => { setError(e.message); setLoading(false); });
  }

  useEffect(() => {
    setOffset(0);
    if (debounce.current) clearTimeout(debounce.current);
    debounce.current = setTimeout(() => load(0, q, kind, lang), q ? 300 : 0);
    return () => { if (debounce.current) clearTimeout(debounce.current); };
  }, [q, kind, lang]);

  function goPage(dir: 1 | -1) {
    const next = offset + dir * LIMIT;
    setOffset(next);
    load(next, q, kind, lang);
  }

  const page = Math.floor(offset / LIMIT) + 1;
  const pages = Math.ceil(total / LIMIT) || 1;

  const filterParts: string[] = [];
  if (q) filterParts.push(`"${q}"`);
  if (kind) filterParts.push(kind);
  if (lang) filterParts.push(lang);
  const detail = `${nodes.length.toLocaleString()} shown · ${total.toLocaleString()} total · p${page}/${pages}${filterParts.length ? ` · ${filterParts.join(' · ')}` : ''}`;
  usePageContext('Nodes', detail);

  return (
    <>
      <div className="page-header">
        <h1 className="page-title">Nodes</h1>
        <span className="count-label">{total.toLocaleString()} total</span>
      </div>

      <div className="filter-row">
        <input
          className="filter-input"
          type="search"
          placeholder="Search symbols…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
        <select
          className="filter-select"
          value={kind}
          onChange={(e) => setKind(e.target.value)}
          aria-label="Filter by kind"
        >
          <option value="">All kinds</option>
          {KIND_OPTIONS.filter(Boolean).map((k) => (
            <option key={k} value={k}>{k}</option>
          ))}
        </select>
        <input
          className="filter-input"
          style={{ maxWidth: 120 }}
          type="text"
          placeholder="Language…"
          value={lang}
          onChange={(e) => setLang(e.target.value)}
        />
      </div>

      {error && <div className="state-msg"><strong>Error</strong>{error}</div>}

      {loading ? (
        <div className="loading-row">Loading…</div>
      ) : nodes.length === 0 ? (
        <div className="state-msg"><strong>No nodes found</strong>Try a different search or filter.</div>
      ) : (
        <div className="list">
          {nodes.map((n) => (
            <div
              key={n.id}
              className={`list-item${selectedId === n.id ? ' selected' : ''}`}
              onClick={() => setSelectedId(n.id)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => { if (e.key === 'Enter') setSelectedId(n.id); }}
            >
              <span className="list-item-icon">{n.kind.slice(0, 4)}</span>
              <div className="list-item-body">
                <div className="list-item-name">{n.name}</div>
                <div className="list-item-sub">{n.file_path}:{n.start_line}</div>
              </div>
              <span className="list-item-badge">{n.language}</span>
              {n.is_exported ? <span className="list-item-badge">pub</span> : null}
            </div>
          ))}
        </div>
      )}

      <div className="pagination">
        <button className="btn" onClick={() => goPage(-1)} disabled={offset === 0}>← Prev</button>
        <span className="page-info">Page {page} of {pages}</span>
        <button className="btn" onClick={() => goPage(1)} disabled={offset + LIMIT >= total}>Next →</button>
      </div>

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
