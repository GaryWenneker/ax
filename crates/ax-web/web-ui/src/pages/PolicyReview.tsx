import MobileControls from '../components/ui/MobileControls';
import ResponsiveDetails from '../components/ui/ResponsiveDetails';
import { useEffect, useState } from 'react';
import { useLive, useNewKeys } from '../lib/useLive';
import { approvePolicyReview, fetchPolicyReview, rejectPolicyReview } from '../policyApi';
import type { PendingPolicyItem } from '../policyTypes';
import { PageCard, PageCardBody, PageEmpty, PageHero, PageLoading, PageShell, PageStack, PageToasts } from '../components/ui/PageLayout';
import {ColumnHeader, ColumnTools, useListColumns} from '../components/ui/ListColumns';
import ModalShell from '../components/ModalShell';
import { usePageContext } from '../context/UiContext';

const canCompare = (item: PendingPolicyItem) => ['new', 'duplicate', 'replacement'].includes(item.effect ?? '') && typeof item.incomingContent === 'string' && (item.effect === 'new' ? item.currentContent === null : typeof item.currentContent === 'string');
const keyOf = (item: PendingPolicyItem) => `${item.kind}:${item.id}`;
const effectLabel = (item: PendingPolicyItem) => ({ new: 'New item', duplicate: 'Duplicate', replacement: 'Replaces existing content' }[item.effect ?? 'new']);

export default function PolicyReviewPage() {
  const [items, setItems] = useState<PendingPolicyItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [serverVersion, setServerVersion] = useState<string | undefined>();
  const [message, setMessage] = useState('');
  const [busy, setBusy] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [kind, setKind] = useState('all');
  const [effect, setEffect] = useState('all');
  const [search, setSearch] = useState('');
  const [confirmation, setConfirmation] = useState<{ action: 'approve' | 'reject'; items: PendingPolicyItem[] } | null>(null);
  usePageContext('Review', !loading ? `${items.length} pending` : undefined);
  async function reload(quiet = false, clearError = true) {
    if (!quiet) setLoading(true);
    try {
      const res = await fetchPolicyReview();
      setItems(res.items);
      setServerVersion(res.serverVersion);
      setSelected(prev => new Set([...prev].filter(key => res.items.some(item => keyOf(item) === key))));
      if (clearError) setError('');
    } catch (e) { setError(e instanceof Error ? e.message : 'Failed to load review queue'); }
    finally { setLoading(false); }
  }
  useEffect(() => { void reload(); }, []);
  useLive(['rules', 'skills'], () => { if (!busy && !confirmation) void reload(true); });
  const fresh = useNewKeys(loading ? null : items.map(keyOf));
  const columns = useListColumns([{label:'Select',header:'✓',sortable:false,filterable:false,minWidth:20},{label:'ID'},{label:'Type / scope'},{label:'Effect'},{label:'Actions',sortable:false,filterable:false}], 'review');
  const filtered = items.filter(item => (kind === 'all' || item.kind === kind)
    && (effect === 'all' || item.effect === effect)
    && `${item.id} ${item.levelOrDescription}`.toLowerCase().includes(search.toLowerCase()));
  const visible = columns.order(filtered.map(item => ({item, values:['',item.id,`${item.kind} ${item.scope ?? ''} ${item.levelOrDescription}`,canCompare(item) ? effectLabel(item) : 'Comparison unavailable','']}))).map(row => row.item);
  const chosen = visible.filter(item => selected.has(keyOf(item)));
  function toggle(item: PendingPolicyItem) {
    setSelected(prev => { const next = new Set(prev); const key = keyOf(item); if (next.has(key)) next.delete(key); else next.add(key); return next; });
  }
  async function decide() {
    if (!confirmation) return;
    setBusy(true); setError('');
    let completed = 0;
    try {
      for (const item of confirmation.items) {
        if (confirmation.action === 'approve') await approvePolicyReview(item.id, item.kind, item);
        else await rejectPolicyReview(item.id, item.kind, item);
        completed++;
        setItems(prev => prev.filter(row => keyOf(row) !== keyOf(item)));
        setSelected(prev => { const next = new Set(prev); next.delete(keyOf(item)); return next; });
      }
      setMessage(`${completed} item${completed === 1 ? '' : 's'} ${confirmation.action === 'approve' ? 'approved' : 'rejected'}.`);
    } catch (e) { setError(`${completed} completed. Remaining items were not processed. ${e instanceof Error ? e.message : 'Review failed'}`); }
    finally { setBusy(false); setConfirmation(null); void reload(true, false); }
  }
  return <PageShell className="review-page">
    <PageHero title="Review" subtitle="Inspect incoming policy changes before activating them. Replica data synchronization runs separately from this policy import queue."
      actions={<button className="btn" disabled={busy} onClick={() => void reload()}>Refresh</button>} />
    <PageToasts err={error || null} ok={message || null} />
    {!loading && items.some(item => !canCompare(item)) && <p role="alert" className="review-compatibility-warning">This review server{serverVersion ? ` (Ax ${serverVersion})` : ''} does not provide full policy comparisons for every item. Update and restart the Ax process serving this page, then Refresh. Approval stays disabled for unverified items. Updating only the primary does not repair an outdated review server.</p>}
    {loading ? <PageLoading label="Loading review queue…" /> : <PageStack><PageCard title="Pending changes" description={`${items.length} awaiting a decision. Approval activates incoming policy; rejection keeps current policy.`}><PageCardBody>
      <MobileControls className="review-toolbar">
        <input className="settings-input" aria-label="Search review items" placeholder="Search ID or description" value={search} onChange={e => setSearch(e.target.value)} />
        <select className="settings-select" aria-label="Review kind" value={kind} onChange={e => setKind(e.target.value)}><option value="all">All types</option><option value="rule">Rules</option><option value="skill">Skills</option></select>
        <select className="settings-select" aria-label="Review effect" value={effect} onChange={e => setEffect(e.target.value)}><option value="all">All changes</option><option value="new">New items</option><option value="duplicate">Duplicates</option><option value="replacement">Replacements</option></select>
        <button className="btn" disabled={busy || !visible.length} onClick={() => setSelected(prev => new Set([...prev, ...visible.map(keyOf)]))}>Select filtered ({visible.length})</button>
        <button className="btn" disabled={busy || !selected.size} onClick={() => setSelected(new Set())}>Clear selection</button>
      </MobileControls>
      <div className="review-toolbar">
        <span>{chosen.length} selected in this filter</span>
        <button className="btn primary" disabled={busy || !chosen.length || chosen.some(item => !canCompare(item))} onClick={() => setConfirmation({ action: 'approve', items: chosen })}>Approve selected</button>
        <button className="btn" disabled={busy || !chosen.length} onClick={() => setConfirmation({ action: 'reject', items: chosen })}>Reject selected</button>
        <button className="btn" disabled={busy || !visible.length || visible.some(item => !canCompare(item))} onClick={() => setConfirmation({ action: 'approve', items: visible })}>Approve filtered type</button>
      </div>
      {!visible.length ? <PageEmpty title={items.length ? 'No matching items' : 'Queue empty'}>No pending rules or skills in this filter.</PageEmpty> : <div className="review-column-list" style={columns.style}><ColumnTools controls={columns} /><ColumnHeader controls={columns} className="review-item-heading" /><div className="review-list">{visible.map(item => <article key={keyOf(item)} className={`review-item${fresh.has(keyOf(item)) ? ' live-new' : ''}`}>
        <div className="review-item-heading"><input type="checkbox" aria-label={`Select ${item.kind} ${item.id}`} disabled={busy} checked={selected.has(keyOf(item))} onChange={() => toggle(item)} /><strong title={item.id}>{item.id}</strong><span>{item.kind} · {item.scope ?? 'Scope unavailable'}{item.kind === 'rule' && ['CRITICAL', 'WARNING', 'INFO'].includes(item.levelOrDescription) ? ` · ${item.levelOrDescription}` : ''}</span><span className="review-effect">{canCompare(item) ? effectLabel(item) : 'Comparison unavailable'}</span><div className="review-actions"><button className="btn primary" disabled={busy || !canCompare(item)} onClick={() => setConfirmation({ action: 'approve', items: [item] })}>Approve</button><button className="btn" disabled={busy} onClick={() => setConfirmation({ action: 'reject', items: [item] })}>Reject</button></div></div>
        <ResponsiveDetails title="Inspect content and source">
        <p>{item.levelOrDescription}</p>
        {!!item.changedFields?.length && <p><strong>Changed:</strong> {item.changedFields.join(', ')}</p>}
        <p>{item.effect === 'new' ? 'No current item with this identity exists. Approval adds it.' : item.effect === 'duplicate' ? 'Active content and policy settings match, ignoring review status. Approval clears the pending copy.' : item.effect === 'replacement' ? 'An active item has this identity with different content or settings. Approval saves the incoming version in its configured scope and storage.' : 'The server has not supplied a verified comparison for this item.'}</p>
<p>{item.origin ?? 'Origin was not recorded'} · Queue file: {item.path}</p><div className="review-comparison"><section><h3>Current</h3><pre>{item.currentContent ?? (canCompare(item) ? 'No current item' : 'Current content was not supplied by this server')}</pre></section><section><h3>Incoming</h3><pre>{item.incomingContent ?? `Preview only — full incoming content was not supplied.\n${item.preview}`}</pre></section></div></ResponsiveDetails>

      </article>)}</div></div>}
    </PageCardBody></PageCard></PageStack>}
    {confirmation && <ModalShell title={`${confirmation.action === 'approve' ? 'Approve' : 'Reject'} ${confirmation.items.length} item${confirmation.items.length === 1 ? '' : 's'}?`} onClose={() => { if (!busy) setConfirmation(null); }} footer={<><button className="btn" disabled={busy} onClick={() => setConfirmation(null)}>Cancel</button><button className="btn primary" disabled={busy} onClick={() => void decide()}>{busy ? 'Processing…' : 'Confirm decision'}</button></>}>
      <p>{confirmation.action === 'approve' ? 'Incoming versions will become active. Review replacements carefully.' : 'Pending versions will be discarded. Current active content stays in place.'}</p><ul>{confirmation.items.map(item => <li key={keyOf(item)}>{item.kind}: {item.id} — {canCompare(item) ? effectLabel(item) : 'Comparison unavailable'}</li>)}</ul>
    </ModalShell>}
  </PageShell>;
}
