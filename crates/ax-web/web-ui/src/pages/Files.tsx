import { useEffect, useRef, useState } from 'react';
import { fetchFiles } from '../api';
import { usePageContext } from '../context/UiContext';
import type { FileRow } from '../types';

const LIMIT = 50;

function formatBytes(b: number) {
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${(b / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(ts: number) {
  return new Date(ts).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
}

export default function FilesPage() {
  const [files, setFiles] = useState<FileRow[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [q, setQ] = useState('');
  const [lang, setLang] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const debounce = useRef<ReturnType<typeof setTimeout> | null>(null);

  function load(newOffset: number, newQ: string, newLang: string) {
    setLoading(true);
    setError(null);
    fetchFiles({ q: newQ, lang: newLang || undefined, limit: LIMIT, offset: newOffset })
      .then((page) => { setFiles(page.files); setTotal(page.total); setLoading(false); })
      .catch((e: Error) => { setError(e.message); setLoading(false); });
  }

  useEffect(() => {
    setOffset(0);
    if (debounce.current) clearTimeout(debounce.current);
    debounce.current = setTimeout(() => load(0, q, lang), q ? 300 : 0);
    return () => { if (debounce.current) clearTimeout(debounce.current); };
  }, [q, lang]);

  function goPage(dir: 1 | -1) {
    const next = offset + dir * LIMIT;
    setOffset(next);
    load(next, q, lang);
  }

  const page = Math.floor(offset / LIMIT) + 1;
  const pages = Math.ceil(total / LIMIT) || 1;

  const fileDetail = `${files.length} shown · ${total.toLocaleString()} total · p${page}/${pages}${q ? ` · "${q}"` : ''}${lang ? ` · ${lang}` : ''}`;
  usePageContext('Files', fileDetail);

  return (
    <>
      <div className="page-header">
        <h1 className="page-title">Files</h1>
        <span className="count-label">{total.toLocaleString()} indexed</span>
      </div>

      <div className="filter-row">
        <input
          className="filter-input"
          type="search"
          placeholder="Filter by path…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
        <input
          className="filter-input"
          style={{ maxWidth: 140 }}
          type="text"
          placeholder="Language…"
          value={lang}
          onChange={(e) => setLang(e.target.value)}
        />
      </div>

      {error && <div className="state-msg"><strong>Error</strong>{error}</div>}

      {loading ? (
        <div className="loading-row">Loading…</div>
      ) : files.length === 0 ? (
        <div className="state-msg"><strong>No files found</strong>Try a different filter.</div>
      ) : (
        <div className="list">
          {files.map((f) => (
            <div key={f.path} className="list-item" style={{ cursor: 'default' }}>
              <div className="list-item-body">
                <div className="list-item-name">{f.path}</div>
                <div className="list-item-sub">
                  {f.node_count} nodes · {formatBytes(f.size)} · indexed {formatDate(f.indexed_at)}
                </div>
              </div>
              <span className="list-item-badge">{f.language}</span>
            </div>
          ))}
        </div>
      )}

      <div className="pagination">
        <button className="btn" onClick={() => goPage(-1)} disabled={offset === 0}>← Prev</button>
        <span className="page-info">Page {page} of {pages}</span>
        <button className="btn" onClick={() => goPage(1)} disabled={offset + LIMIT >= total}>Next →</button>
      </div>
    </>
  );
}
