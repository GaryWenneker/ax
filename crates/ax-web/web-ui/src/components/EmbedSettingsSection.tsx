import { useCallback, useEffect, useState } from 'react';

type EmbedStatus = {
  backend: string;
  model: string;
};

/** Settings → Embeddings: the one hash model from GET /api/memory/embed-status. */
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
          model: d.model ?? 'hash-v1',
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
      <div className="settings-row">
        <div className="settings-row-label">
          <span className="settings-row-title">Memory embeddings</span>
          <span className="settings-row-desc">
            {err
              ? err
              : loading
                ? 'Checking the embedding model…'
                : 'Memory search uses one local hash model. Nothing is downloaded.'}
          </span>
        </div>
        <div className="settings-row-control settings-row-control--actions">
          {status && (
            <span className="settings-embed-badge settings-embed-badge--hash" title={status.model}>
              {status.model}
            </span>
          )}
          <button type="button" className="btn btn-subtle" disabled={loading} onClick={load}>
            {loading ? 'Checking…' : 'Refresh'}
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
        <dl className="settings-meta">
          <div className="settings-meta-row">
            <dt>Backend</dt>
            <dd className="mono">{status.backend}</dd>
          </div>
          <div className="settings-meta-row">
            <dt>Model</dt>
            <dd className="mono">{status.model}</dd>
          </div>
        </dl>
      )}

      <p className="settings-callout settings-embed-docs">
        Docs:{' '}
        <a href="https://getax.wenneker.io/guides/memory/" target="_blank" rel="noreferrer">
          Memory vault
        </a>
      </p>
    </>
  );
}
