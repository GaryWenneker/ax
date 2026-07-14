import { useEffect, useState } from 'react';
import { fetchNodeDetail, fetchSource } from '../api';
import type { SourceSlice } from '../api';
import Codicon from './Codicon';
import SourceViewer from './SourceViewer';
import { Spinner } from './ui/Spinner';
import { PreviewZoomToolbar, loadPreviewScale } from './PreviewZoom';
import { ResizableBlade } from './BladeResize';
import type { NodeDetail } from '../types';

interface Props {
  nodeId: string;
  onClose: () => void;
  onNavigate: (id: string) => void;
  /** Inline blade beside sibling panes (default) or fullscreen overlay. */
  variant?: 'blade' | 'overlay';
}

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

export default function NodeDetailPanel({
  nodeId,
  onClose,
  onNavigate,
  variant = 'blade',
}: Props) {
  const [detail, setDetail] = useState<NodeDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [previewScale, setPreviewScale] = useState(loadPreviewScale);
  const [source, setSource] = useState<SourceSlice | null>(null);
  const [sourceError, setSourceError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setDetail(null);
    setSource(null);
    setSourceError(null);
    fetchNodeDetail(nodeId)
      .then((d) => {
        if (cancelled) return;
        setDetail(d);
        setLoading(false);
        fetchSource({
          path: d.node.file_path,
          start: d.node.start_line,
          end: d.node.end_line,
        })
          .then((s) => { if (!cancelled) setSource(s); })
          .catch((e: Error) => { if (!cancelled) setSourceError(e.message); });
      })
      .catch((e: Error) => {
        if (!cancelled) { setError(e.message); setLoading(false); }
      });
    return () => { cancelled = true; };
  }, [nodeId]);

  const node = detail?.node;

  const panel = (
    <div
      className={`detail-panel${variant === 'blade' ? ' detail-panel--blade' : ''}`}
      role={variant === 'overlay' ? 'dialog' : 'complementary'}
      aria-label={node?.name ?? 'Symbol detail'}
    >
      <div className="detail-header">
        <span className="detail-title">
          {node && <Codicon name={KIND_ICONS[node.kind] ?? 'symbol-misc'} className="detail-title-icon" />}
          {node?.name ?? nodeId}
        </span>
        <button type="button" className="detail-close" onClick={onClose} aria-label="Close">
          <Codicon name="close" />
        </button>
      </div>

      <PreviewZoomToolbar scale={previewScale} onScaleChange={setPreviewScale} />

      <div
        className="detail-body"
        style={{ fontSize: `calc(var(--fs-sm) * ${previewScale})` }}
      >
        {loading && (
          <div className="loading-row">
            <Spinner />
            Loading…
          </div>
        )}
        {error && <div className="state-msg"><strong>Error</strong> {error}</div>}

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

            <div>
              <div className="detail-section-title">
                Source
                {source && (
                  <span className="detail-section-meta"> {source.path}:{source.from}–{source.to}</span>
                )}
              </div>
              {sourceError && <div className="empty-label">Source unavailable: {sourceError}</div>}
              {!sourceError && !source && (
                <div className="loading-row">
                  <Spinner />
                  Loading source…
                </div>
              )}
              {source && (
                <SourceViewer
                  lines={source.lines}
                  language={node.language}
                  highlightRange={{ start: node.start_line, end: node.end_line }}
                />
              )}
            </div>

            {detail.callers.length > 0 && (
              <div>
                <div className="detail-section-title">Callers ({detail.callers.length})</div>
                <div className="edge-list">
                  {detail.callers.map((c) => (
                    <button
                      key={`${c.id}-${c.edge_kind}`}
                      type="button"
                      className="edge-item"
                      onClick={() => onNavigate(c.id)}
                    >
                      <Codicon name={KIND_ICONS[c.kind] ?? 'symbol-misc'} className="edge-item-icon" />
                      <span className="edge-name">{c.name}</span>
                      {c.edge_confidence && (
                        <span className={`confidence-badge confidence-${c.edge_confidence}`}>{c.edge_confidence}</span>
                      )}
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
                      type="button"
                      className="edge-item"
                      onClick={() => onNavigate(c.id)}
                    >
                      <Codicon name={KIND_ICONS[c.kind] ?? 'symbol-misc'} className="edge-item-icon" />
                      <span className="edge-name">{c.name}</span>
                      {c.edge_confidence && (
                        <span className={`confidence-badge confidence-${c.edge_confidence}`}>{c.edge_confidence}</span>
                      )}
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
  );

  if (variant === 'overlay') {
    return (
      <div
        className="detail-overlay"
        role="presentation"
        onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
      >
        {panel}
      </div>
    );
  }

  if (variant === 'blade') {
    return <ResizableBlade>{panel}</ResizableBlade>;
  }

  return panel;
}
