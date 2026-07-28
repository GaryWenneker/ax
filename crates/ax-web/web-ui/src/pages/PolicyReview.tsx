import { useEffect, useState } from 'react';
import {
  approvePolicyReview,
  fetchPolicyReview,
  rejectPolicyReview,
} from '../policyApi';
import type { PendingPolicyItem } from '../policyTypes';
import {
  DataTable,
  PageCard,
  PageCardBody,
  PageEmpty,
  PageHero,
  PageLoading,
  PageShell,
  PageStack,
  PageToasts,
} from '../components/ui/PageLayout';
import { usePageContext } from '../context/UiContext';

export default function PolicyReviewPage() {
  const [items, setItems] = useState<PendingPolicyItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [busyId, setBusyId] = useState<string | null>(null);

  usePageContext('Review', !loading ? `${items.length} pending` : undefined);

  async function reload() {
    setLoading(true);
    try {
      const res = await fetchPolicyReview();
      setItems(res.items);
      setError('');
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load review queue');
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void reload();
  }, []);

  async function approve(id: string) {
    setBusyId(id);
    try {
      await approvePolicyReview(id);
      setItems((prev) => prev.filter((i) => i.id !== id));
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Approve failed');
    } finally {
      setBusyId(null);
    }
  }

  async function reject(id: string) {
    setBusyId(id);
    try {
      await rejectPolicyReview(id);
      setItems((prev) => prev.filter((i) => i.id !== id));
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Reject failed');
    } finally {
      setBusyId(null);
    }
  }

  if (loading) {
    return (
      <PageShell>
        <PageHero title="Review" subtitle="Pending pack imports awaiting approval." />
        <PageLoading label="Loading review queue…" />
      </PageShell>
    );
  }

  return (
    <PageShell>
      <PageHero
        title="Review"
        subtitle="Approve or reject incoming shared rules and skills (when requireReview is on)."
        actions={
          <button type="button" className="btn btn-subtle" onClick={() => void reload()}>
            Refresh
          </button>
        }
      />
      <PageToasts err={error || null} />
      <PageStack>
        <PageCard title="Pending" description="Items staged under .ax/policy/pending/.">
          <PageCardBody>
            {items.length === 0 ? (
              <PageEmpty title="Queue empty">No pending rules or skills.</PageEmpty>
            ) : (
              <DataTable dense>
                <thead>
                  <tr>
                    <th>Kind</th>
                    <th>ID</th>
                    <th>Summary</th>
                    <th className="col-actions">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((item) => (
                    <tr key={`${item.kind}:${item.id}`}>
                      <td>{item.kind}</td>
                      <td className="mono">{item.id}</td>
                      <td>{item.levelOrDescription}</td>
                      <td className="col-actions">
                        <button
                          type="button"
                          className="btn primary"
                          disabled={busyId === item.id}
                          onClick={() => void approve(item.id)}
                        >
                          Approve
                        </button>{' '}
                        <button
                          type="button"
                          className="btn btn-subtle"
                          disabled={busyId === item.id}
                          onClick={() => void reject(item.id)}
                        >
                          Reject
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </DataTable>
            )}
          </PageCardBody>
        </PageCard>
      </PageStack>
    </PageShell>
  );
}
