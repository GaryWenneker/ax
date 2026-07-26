import { useCallback, useEffect, useState } from 'react';

type EmbedStatus = {
  backend: string;
  tokenizer: boolean;
  feature: boolean;
  modelPath?: string | null;
  tokenizerPath?: string | null;
};

function backendLabel(backend: string): string {
  switch (backend) {
    case 'onnx':
      return 'ONNX (dense)';
    case 'onnx_unconfigured':
      return 'ONNX model found, runtime not ready';
    case 'hash':
    default:
      return 'Feature hash (default)';
  }
}

function backendHint(s: EmbedStatus): string {
  if (!s.feature) {
    return 'This ax binary was built without the onnx Cargo feature. Rebuild with --features onnx (or the CLI onnx feature) to enable dense vectors.';
  }
  if (s.backend === 'onnx') {
    return 'Memory recall uses ONNX dense embeddings. Keep tokenizer.json beside the model for accurate WordPiece tokenization.';
  }
  if (s.backend === 'onnx_unconfigured') {
    return 'A model path is configured but the runtime probe failed — check ORT / model compatibility, then restart ax web.';
  }
  return 'No ONNX model found. Place all-MiniLM-L6-v2.onnx under ~/.ax/models/ or set AX_ONNX_MODEL. Until then, memory uses feature-hash embeddings.';
}

/** Settings → Embeddings: status from GET /api/memory/embed-status. */
export default function EmbedSettingsSection() {
  const [status, setStatus] = useState<EmbedStatus | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tick, setTick] = useState(0);

  const load = useCallback(() => {
    setTick((t) => t + 1);
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    fetch('/api/memory/embed-status')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((d: EmbedStatus) => {
        if (cancelled) return;
        setStatus({
          backend: d.backend ?? 'hash',
          tokenizer: !!d.tokenizer,
          feature: !!d.feature,
          modelPath: d.modelPath ?? null,
          tokenizerPath: d.tokenizerPath ?? null,
        });
        setErr(null);
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setErr(e instanceof Error ? e.message : 'Failed to load embed status');
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [tick]);

  return (
    <>
      <div className="setting-row">
        <div>
          <div className="setting-row-title">Memory embeddings</div>
          <div className="setting-row-desc">
            {status
              ? backendHint(status)
              : err
                ? err
                : loading
                  ? 'Checking embed backend…'
                  : '—'}
          </div>
        </div>
        <div className="setting-row-control" style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          {status && (
            <span
              className={`settings-embed-badge settings-embed-badge--${status.backend}`}
              title={`backend=${status.backend}`}
            >
              {backendLabel(status.backend)}
            </span>
          )}
          <button type="button" className="btn" disabled={loading} onClick={load}>
            {loading ? 'Probing…' : 'Re-probe'}
          </button>
        </div>
      </div>

      {err && (
        <p className="settings-inline-err" role="alert">
          {err}{' '}
          <button type="button" className="status-panel-link" onClick={load}>
            Retry
          </button>
        </p>
      )}

      {status && (
        <div className="page-table-wrap settings-embed-table-wrap">
          <table className="page-table page-table--dense">
            <tbody>
              <tr>
                <th scope="row">Backend</th>
                <td className="mono">{status.backend}</td>
              </tr>
              <tr>
                <th scope="row">onnx feature</th>
                <td>
                  <span className={status.feature ? 'lsp-server-badge--ok' : 'lsp-server-badge--miss'}>
                    {status.feature ? 'enabled' : 'disabled in this binary'}
                  </span>
                </td>
              </tr>
              <tr>
                <th scope="row">Model</th>
                <td className="mono settings-plugin-entry" title={status.modelPath ?? ''}>
                  {status.modelPath || '—'}
                </td>
              </tr>
              <tr>
                <th scope="row">Tokenizer</th>
                <td className="mono settings-plugin-entry" title={status.tokenizerPath ?? ''}>
                  {status.tokenizer
                    ? status.tokenizerPath || 'configured'
                    : 'missing (hashed token ids)'}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      )}

      <p className="status-panel-muted settings-embed-docs">
        Docs:{' '}
        <a href="https://getax.wenneker.io/guides/memory/" target="_blank" rel="noreferrer">
          Memory vault
        </a>
        {' · '}
        <code>docs/ONNX.md</code>
        {' · '}
        env <code>AX_ONNX_MODEL</code> / <code>AX_ONNX_TOKENIZER</code>
      </p>
    </>
  );
}
