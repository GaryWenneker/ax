import { useCallback, useEffect, useState } from 'react';
import Codicon from './Codicon';
import { ItemList } from './ui/ItemList';
import { ItemRow } from './ui/PageLayout';
import { canStartReplay, uncertaintyLabel, type ManifestSummary } from '../lib/manifestAuthority';
import { formatReplayNotice, type ReplayRunView } from '../lib/replayScorecard';

type Payload = {
  manifests?: ManifestSummary[];
  error?: string;
};

export default function ManifestAuthorityPanel() {
  const [manifests, setManifests] = useState<ManifestSummary[]>([]);
  const [error, setError] = useState('');
  const [loaded, setLoaded] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [lastRun, setLastRun] = useState<ReplayRunView | null>(null);

  const reload = useCallback(async () => {
    try {
      const response = await fetch('/api/context-authority/manifests');
      const data = (await response.json()) as Payload;
      setError(data.error ? String(data.error) : '');
      setManifests(Array.isArray(data.manifests) ? data.manifests : []);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not load manifests');
      setManifests([]);
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const response = await fetch('/api/context-authority/manifests');
        const data = (await response.json()) as Payload;
        if (cancelled) return;
        setError(data.error ? String(data.error) : '');
        setManifests(Array.isArray(data.manifests) ? data.manifests : []);
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : 'Could not load manifests');
          setManifests([]);
        }
      } finally {
        if (!cancelled) setLoaded(true);
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  async function startReplay(row: ManifestSummary) {
    if (!canStartReplay(row, busyId)) return;
    setBusyId(row.id);
    setError('');
    try {
      const response = await fetch('/api/context-authority/replay', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ manifestId: row.id }),
      });
      const data = (await response.json()) as {
        ok?: boolean;
        error?: string;
        run?: ReplayRunView;
      };
      if (!data.ok || !data.run) {
        setError(String(data.error || 'Replay failed'));
        return;
      }
      setLastRun(data.run);
      await reload();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Replay failed');
    } finally {
      setBusyId(null);
    }
  }

  return (
    <section className="specs-authority" aria-label="Context manifests">
      <div className="specs-authority-head">
        <h2 className="specs-authority-title">Context authority</h2>
        <p className="specs-authority-copy">
          Manifest approval and replay state. Uncertainty stays visible; there is no confident green without evidence.
        </p>
      </div>
      {error ? <p role="alert">{error}</p> : null}
      {lastRun ? (
        <p className="specs-authority-scorecard" role="status" aria-label="Replay scorecard">
          {formatReplayNotice(lastRun)}
        </p>
      ) : null}
      {!loaded ? <p role="status">Loading manifests…</p> : null}
      {loaded && !error && manifests.length === 0 ? (
        <p role="status">No sealed manifests yet.</p>
      ) : null}
      {manifests.length > 0 ? (
        <ItemList
          className="calm-list specs-authority-list"
          listId="context-manifests"
          columns={[
            { label: 'Manifest' },
            { label: 'Session / turn' },
            { label: 'Uncertainty' },
            { label: 'Actions', sortable: false, filterable: false },
          ]}
        >
          {manifests.map((row) => {
            const labels =
              row.uncertainties.length > 0
                ? row.uncertainties.map(uncertaintyLabel)
                : ['No open uncertainty'];
            return (
              <ItemRow
                key={row.id}
                static
                variant="graph"
                title={`${row.session_id} · ${row.turn_id}`}
                subtitle={`agent ${row.agent_id}`}
                badges={
                  <>
                    <span className="page-item-badge">
                      <Codicon
                        name={row.approval === 'approved' ? 'check' : 'warning'}
                        className="badge-icon"
                      />
                      {row.approval}
                    </span>
                    <span className="page-item-badge">
                      <Codicon
                        name={row.integrity_ok ? 'shield' : 'error'}
                        className="badge-icon"
                      />
                      {row.integrity_ok ? 'seal ok' : 'seal broken'}
                    </span>
                    <span className="page-item-badge">
                      <Codicon name="debug-restart" className="badge-icon" />
                      {row.replay_status.replace(/_/g, ' ')}
                    </span>
                    {labels.map((label) => (
                      <span key={label} className="page-item-badge">
                        <Codicon
                          name={row.uncertainties.length > 0 ? 'warning' : 'info'}
                          className="badge-icon"
                        />
                        {label}
                      </span>
                    ))}
                  </>
                }
                aside={
                  <button
                    type="button"
                    className="btn btn-compact"
                    disabled={!canStartReplay(row, busyId)}
                    aria-label={`Replay ${row.session_id} ${row.turn_id}`}
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      void startReplay(row);
                    }}
                  >
                    {busyId === row.id ? 'Starting…' : 'Replay'}
                  </button>
                }
                listValues={[row.session_id, `${row.session_id}/${row.turn_id}`, labels.join(' '), 'Replay']}
              />
            );
          })}
        </ItemList>
      ) : null}
    </section>
  );
}
