import { useEffect, useState } from 'react';
import { fetchNodes, fetchSource } from '../api';
import type { SourceSlice } from '../api';
import Codicon from './Codicon';
import FileTypeIcon from './FileTypeIcon';
import SymbolOutline from './SymbolOutline';
import { PreviewZoomToolbar, loadPreviewScale } from './PreviewZoom';
import type { FileRow, NodeRow } from '../types';

function formatBytes(b: number) {
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${(b / (1024 * 1024)).toFixed(1)} MB`;
}

interface Props {
  file: FileRow;
  onClose: () => void;
  onNodeSelect: (id: string) => void;
  selectedNodeId?: string | null;
}

export default function FilePreview({ file, onClose, onNodeSelect, selectedNodeId }: Props) {
  const [nodes, setNodes] = useState<NodeRow[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [previewScale, setPreviewScale] = useState(loadPreviewScale);
  const [tab, setTab] = useState<'symbols' | 'source'>('symbols');
  const [source, setSource] = useState<SourceSlice | null>(null);
  const [sourceError, setSourceError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    setError(null);
    setNodes([]);
    setSource(null);
    setSourceError(null);
    fetchNodes({ file: file.path, limit: 2000, offset: 0 })
      .then((page) => {
        setNodes(page.nodes);
        setTotal(page.total);
        setLoading(false);
      })
      .catch((e: Error) => {
        setError(e.message);
        setLoading(false);
      });
  }, [file.path]);

  useEffect(() => {
    if (tab !== 'source' || source || sourceError) return;
    fetchSource({ path: file.path, start: 1, end: 500, context: 0 })
      .then(setSource)
      .catch((e: Error) => setSourceError(e.message));
  }, [tab, file.path, source, sourceError]);

  const fileName = file.path.split('/').pop() ?? file.path;

  return (
    <div className="files-preview-pane">
      <div className="files-preview-toolbar">
        <PreviewZoomToolbar
          scale={previewScale}
          onScaleChange={setPreviewScale}
          label="Index preview"
        />
        <button type="button" className="files-preview-close" onClick={onClose} aria-label="Close preview">
          <Codicon name="close" />
        </button>
      </div>

      <div
        className="files-preview-body"
        style={{ fontSize: `calc(var(--fs-sm) * ${previewScale})` }}
      >
        <div className="files-preview-header">
          <FileTypeIcon
            fileName={fileName}
            language={file.language}
            size={18}
            className="files-preview-icon"
          />
          <div className="files-preview-header-text">
            <span className="files-preview-path" title={file.path}>{file.path}</span>
            <dl className="files-preview-meta files-preview-meta--inline">
              <div><dt>Language</dt><dd>{file.language}</dd></div>
              <div><dt>Size</dt><dd>{formatBytes(file.size)}</dd></div>
              <div><dt>Nodes</dt><dd>{file.node_count}</dd></div>
              <div><dt>Indexed</dt><dd>{new Date(file.indexed_at).toLocaleString()}</dd></div>
            </dl>
          </div>
        </div>

        <div className="files-preview-tabs" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={tab === 'symbols'}
            className={`files-preview-tab${tab === 'symbols' ? ' files-preview-tab--active' : ''}`}
            onClick={() => setTab('symbols')}
          >
            Symbols ({total.toLocaleString()})
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === 'source'}
            className={`files-preview-tab${tab === 'source' ? ' files-preview-tab--active' : ''}`}
            onClick={() => setTab('source')}
          >
            Source
          </button>
        </div>

        {tab === 'symbols' && (
          <>
            {loading && <div className="page-loading">Loading index…</div>}
            {error && <div className="state-msg"><strong>Error</strong> {error}</div>}

            {!loading && !error && nodes.length > 0 && (
              <>
                <SymbolOutline nodes={nodes} selectedId={selectedNodeId} onSelect={onNodeSelect} />
                {total > nodes.length && (
                  <div className="files-index-truncated">
                    Showing {nodes.length} of {total.toLocaleString()} symbols.
                  </div>
                )}
              </>
            )}

            {!loading && !error && nodes.length === 0 && (
              <div className="files-preview-empty-inline">
                {file.node_count === 0 && file.language === 'unknown'
                  ? 'No symbols were extracted for this file. If this is C# or another supported language, run a full index (ax index) and reopen the file.'
                  : 'No symbols indexed for this file.'}
              </div>
            )}
          </>
        )}

        {tab === 'source' && (
          <>
            {sourceError && (
              <div className="files-preview-empty-inline">Source unavailable: {sourceError}</div>
            )}
            {!sourceError && !source && <div className="page-loading">Loading source…</div>}
            {source && (
              <>
                <pre className="detail-code detail-code--source">
                  {source.lines.map((l) => (
                    <div key={l.no} className="source-line">
                      <span className="source-line-no">{l.no}</span>
                      <span className="source-line-text">{l.text || ' '}</span>
                    </div>
                  ))}
                </pre>
                {source.total_lines > source.to && (
                  <div className="files-index-truncated">
                    Showing lines 1–{source.to} of {source.total_lines}.
                  </div>
                )}
              </>
            )}
          </>
        )}
      </div>
    </div>
  );
}
