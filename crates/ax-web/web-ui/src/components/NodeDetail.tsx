import { useEffect, useState } from 'react';
import { fetchNodeDetail } from '../api';
import type { NodeDetail } from '../types';

interface Props {
  nodeId: string;
  onClose: () => void;
  onNavigate: (id: string) => void;
}

export default function NodeDetailPanel({ nodeId, onClose, onNavigate }: Props) {
  const [detail, setDetail] = useState<NodeDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    setError(null);
    setDetail(null);
    fetchNodeDetail(nodeId)
      .then((d) => { setDetail(d); setLoading(false); })
      .catch((e: Error) => { setError(e.message); setLoading(false); });
  }, [nodeId]);

  const node = detail?.node;

  return (
    <div
      className="detail-overlay"
      role="dialog"
      aria-modal="true"
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div className="detail-panel">
        <div className="detail-header">
          <span className="detail-title">{node?.name ?? nodeId}</span>
          <button className="detail-close" onClick={onClose} aria-label="Close">✕</button>
        </div>

        <div className="detail-body">
          {loading && <div className="loading-row">Loading…</div>}
          {error && <div className="state-msg"><strong>Error</strong>{error}</div>}

          {node && (
            <>
              <div className="detail-meta">
                <div className="detail-kv"><span className="detail-key">Kind</span><span className="detail-val">{node.kind}</span></div>
                <div className="detail-kv"><span className="detail-key">Language</span><span className="detail-val">{node.language}</span></div>
                <div className="detail-kv"><span className="detail-key">File</span><span className="detail-val">{node.file_path}:{node.start_line}</span></div>
                {node.visibility && (
                  <div className="detail-kv"><span className="detail-key">Visibility</span><span className="detail-val">{node.visibility}</span></div>
                )}
                <div className="detail-kv">
                  <span className="detail-key">Flags</span>
                  <span className="detail-val">
                    {[
                      node.is_exported ? 'exported' : null,
                      node.is_async ? 'async' : null,
                    ].filter(Boolean).join(', ') || '—'}
                  </span>
                </div>
              </div>

              {node.signature && (
                <div>
                  <div className="detail-section-title">Signature</div>
                  <pre className="detail-code">{node.signature}</pre>
                </div>
              )}

              {node.docstring && (
                <div>
                  <div className="detail-section-title">Docstring</div>
                  <pre className="detail-code">{node.docstring}</pre>
                </div>
              )}

              {detail.callers.length > 0 && (
                <div>
                  <div className="detail-section-title">Callers ({detail.callers.length})</div>
                  <div className="edge-list">
                    {detail.callers.map((c) => (
                      <button
                        key={`${c.id}-${c.edge_kind}`}
                        className="edge-item"
                        onClick={() => onNavigate(c.id)}
                      >
                        <span className="list-item-icon">{c.kind}</span>
                        <span className="edge-name">{c.name}</span>
                        <span className="edge-meta">:{c.start_line}</span>
                      </button>
                    ))}
                  </div>
                </div>
              )}
              {detail.callers.length === 0 && (
                <div>
                  <div className="detail-section-title">Callers</div>
                  <div className="empty-label">No callers found.</div>
                </div>
              )}

              {detail.callees.length > 0 && (
                <div>
                  <div className="detail-section-title">Callees ({detail.callees.length})</div>
                  <div className="edge-list">
                    {detail.callees.map((c) => (
                      <button
                        key={`${c.id}-${c.edge_kind}`}
                        className="edge-item"
                        onClick={() => onNavigate(c.id)}
                      >
                        <span className="list-item-icon">{c.kind}</span>
                        <span className="edge-name">{c.name}</span>
                        <span className="edge-meta">:{c.start_line}</span>
                      </button>
                    ))}
                  </div>
                </div>
              )}
              {detail.callees.length === 0 && (
                <div>
                  <div className="detail-section-title">Callees</div>
                  <div className="empty-label">No callees found.</div>
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
