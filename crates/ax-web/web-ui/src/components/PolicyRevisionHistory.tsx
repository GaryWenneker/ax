import { useEffect, useState } from 'react';
import ModalShell from './ModalShell';
import { fetchPolicyRevisions, restorePolicyRevision, type PolicyRevisionRow } from '../policyApi';
import { revisionHashPrefix, revisionSourceLabel } from '../policyRevisions';

interface Props {
  kind: 'rule' | 'skill';
  itemId: string;
  onRestored: () => void;
}

export default function PolicyRevisionHistory({ kind, itemId, onRestored }: Props) {
  const [open, setOpen] = useState(false);
  const [rows, setRows] = useState<PolicyRevisionRow[]>([]);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const [restoring, setRestoring] = useState<number | null>(null);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    setError('');
    fetchPolicyRevisions(kind, itemId)
      .then((r) => {
        if (!cancelled) setRows(r.revisions);
      })
      .catch((e: Error) => {
        if (!cancelled) setError(e.message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, kind, itemId]);

  async function restore(id: number) {
    setRestoring(id);
    setError('');
    try {
      await restorePolicyRevision(kind, itemId, id);
      setOpen(false);
      onRestored();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Restore failed');
    } finally {
      setRestoring(null);
    }
  }

  return (
    <>
      <button type="button" className="btn" onClick={() => setOpen(true)}>
        History
      </button>
      {open ? (
        <ModalShell
          title="Revision history"
          subtitle="Hash-on-change snapshots (up to 20). Identical saves are not listed."
          onClose={() => setOpen(false)}
          size="lg"
        >
          {error ? <p className="settings-toast settings-toast--err">{error}</p> : null}
          {loading ? (
            <p className="muted">Loading…</p>
          ) : rows.length === 0 ? (
            <p className="muted">No revisions yet. Saves and accepted package restores after this upgrade appear here.</p>
          ) : (
            <ul className="policy-rev-list">
              {rows.map((row) => (
                <li key={row.id} className="policy-rev-row">
                  <div>
                    <div className="policy-rev-meta">
                      {new Date(row.createdAt).toLocaleString()} · {revisionSourceLabel(row.source)}
                    </div>
                    <div className="mono muted">{revisionHashPrefix(row.contentHash)}</div>
                  </div>
                  <button
                    type="button"
                    className="btn"
                    disabled={restoring !== null}
                    onClick={() => void restore(row.id)}
                  >
                    {restoring === row.id ? 'Restoring…' : 'Restore'}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </ModalShell>
      ) : null}
    </>
  );
}
