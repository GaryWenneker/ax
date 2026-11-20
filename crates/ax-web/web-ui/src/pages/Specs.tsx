import { useEffect, useState } from 'react';
import { ItemList } from '../components/ui/ItemList';
import { ItemRow } from '../components/ui/PageLayout';
import { navigateRoute } from '../lib/routes';

type SpecItem = {
  id: string;
  projectId: string;
  title: string;
  path: string;
  sessions: string;
  approvedVersion: number | null;
  latestStatus: string;
  revisionCount: number;
  kind: string;
};

type Revision = {
  version: number;
  status: string;
  source: string;
  body: string;
  hash: string;
  basedOn: number | null;
  createdAt: number;
};

type KindFilter = 'all' | 'plan' | 'spec';
type StatusFilter = 'all' | 'draft' | 'approved' | 'change_requested';

const KIND_FILTERS: { id: KindFilter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'plan', label: 'Plans' },
  { id: 'spec', label: 'Specs' },
];

const STATUS_FILTERS: { id: StatusFilter; label: string }[] = [
  { id: 'all', label: 'Any status' },
  { id: 'draft', label: 'Drafts' },
  { id: 'approved', label: 'Approved' },
  { id: 'change_requested', label: 'Change requested' },
];

function lineDiff(before: string, after: string): { added: string[]; removed: string[] } {
  const previous = new Set(before.split('\n'));
  const next = new Set(after.split('\n'));
  return {
    removed: before.split('\n').filter((line) => line.length > 0 && !next.has(line)),
    added: after.split('\n').filter((line) => line.length > 0 && !previous.has(line)),
  };
}

export default function SpecsPage() {
  const [specs, setSpecs] = useState<SpecItem[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [revisions, setRevisions] = useState<Revision[]>([]);
  const [openVersion, setOpenVersion] = useState<number | null>(null);
  const [draftText, setDraftText] = useState('');
  const [dirty, setDirty] = useState(false);
  const [revising, setRevising] = useState(false);
  const [kindFilter, setKindFilter] = useState<KindFilter>('all');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');
  const [query, setQuery] = useState('');
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');

  async function load() {
    const response = await fetch('/api/specs');
    const data = await response.json();
    setError(data.error ? String(data.error) : '');
    setSpecs(Array.isArray(data.specs) ? data.specs : []);
  }

  useEffect(() => {
    void load();
  }, []);

  function showVersion(rows: Revision[], version: number | null, keepText: boolean) {
    setOpenVersion(version);
    const row = rows.find((item) => item.version === version);
    if (!row || keepText) return;
    setDraftText(row.body);
    setDirty(false);
    setRevising(row.status === 'draft');
  }

  async function loadRevisions(id: string, version: number | null) {
    setSelected(id);
    const response = await fetch(`/api/specs/${encodeURIComponent(id)}/revisions`);
    const data = await response.json();
    if (data.error) setError(String(data.error));
    const rows: Revision[] = Array.isArray(data.revisions) ? data.revisions : [];
    setRevisions(rows);
    const approved = rows.find((row) => row.status === 'approved');
    const newest = rows.length > 0 ? rows[rows.length - 1].version : null;
    const exists = version != null && rows.some((row) => row.version === version);
    showVersion(rows, exists ? version : approved?.version ?? newest, false);
  }

  async function openSpec(id: string) {
    setNotice('');
    await loadRevisions(id, null);
  }

  function selectVersion(version: number) {
    showVersion(revisions, version, dirty);
  }

  async function approve(version: number) {
    if (!selected || dirty) return;
    const response = await fetch(`/api/specs/${encodeURIComponent(selected)}/approve`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ version }),
    });
    const data = await response.json();
    if (!data.ok) {
      setError(String(data.error || 'Approval failed'));
      return;
    }
    await load();
    await loadRevisions(selected, Number(data.version));
    setNotice(`Approved version ${data.version}. Earlier revisions stay in the history.`);
  }

  async function save() {
    if (!selected || openVersion == null || !dirty) return;
    const response = await fetch(`/api/specs/${encodeURIComponent(selected)}/revisions`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ basedOn: openVersion, body: draftText }),
    });
    const data = await response.json();
    if (!data.ok) {
      setError(String(data.error || 'Save failed'));
      return;
    }
    setDirty(false);
    await load();
    await loadRevisions(selected, Number(data.version));
    setNotice(`Saved draft v${data.version}. The previous body is still in the history.`);
  }

  const current = revisions.find((row) => row.version === openVersion);
  const currentIndex = revisions.findIndex((row) => row.version === openVersion);
  const previous = currentIndex > 0 ? revisions[currentIndex - 1] : null;
  const diff = current && previous ? lineDiff(previous.body, current.body) : null;
  const canEdit = Boolean(current && (current.status === 'draft' || revising));
  const needle = query.trim().toLowerCase();
  const visible = specs.filter((spec) => {
    if (kindFilter === 'plan' && spec.kind !== 'plan') return false;
    if (kindFilter === 'spec' && spec.kind !== 'spec') return false;
    if (statusFilter !== 'all' && spec.latestStatus !== statusFilter) return false;
    if (!needle) return true;
    const haystack = `${spec.title} ${spec.id} ${spec.path} ${spec.projectId} ${spec.sessions}`.toLowerCase();
    return haystack.includes(needle);
  });

  return (
    <section className="page-stack specs-page">
      <header className="page-hero">
        <h1>Specs</h1>
        <p>Approved specs stay the agreement. An edit is saved as the next draft.</p>
      </header>
      <div className="specs-filters" role="group" aria-label="Filter specs">
        {KIND_FILTERS.map((filter) => (
          <button
            key={filter.id}
            type="button"
            className="btn btn-compact"
            aria-pressed={kindFilter === filter.id}
            onClick={() => setKindFilter(filter.id)}
          >
            {filter.label}
          </button>
        ))}
        {STATUS_FILTERS.map((filter) => (
          <button
            key={filter.id}
            type="button"
            className="btn btn-compact"
            aria-pressed={statusFilter === filter.id}
            onClick={() => setStatusFilter(filter.id)}
          >
            {filter.label}
          </button>
        ))}
        <label className="specs-search">
          Search
          <input
            aria-label="Search specs"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
      </div>
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      {dirty && <p role="status">Unsaved changes</p>}
      {specs.length === 0 ? (
        <p role="status">No specs stored yet. A finished plan file or a finished plan request appears here as a draft.</p>
      ) : visible.length === 0 ? (
        <p role="status">
          No specs match these filters.
          <button
            type="button"
            className="btn btn-compact"
            onClick={() => {
              setKindFilter('all');
              setStatusFilter('all');
              setQuery('');
            }}
          >
            Clear filters
          </button>
        </p>
      ) : (
        <ItemList className="calm-list" listId="specs" columns={[{ label: 'Spec' }, { label: 'Path' }, { label: 'Status' }]}>
          {visible.map((spec) => (
            <ItemRow
              key={spec.id}
              variant="graph"
              title={spec.title || spec.id}
              subtitle={[spec.kind, spec.projectId, spec.sessions, spec.approvedVersion == null ? '' : `approved v${spec.approvedVersion}`].filter(Boolean).join(' · ')}
              meta={spec.latestStatus}
              selected={selected === spec.id}
              listValues={[
                spec.title || spec.id,
                `${spec.path} ${spec.projectId} ${spec.sessions}`,
                spec.latestStatus,
              ]}
              onClick={() => void openSpec(spec.id)}
            />
          ))}
        </ItemList>
      )}
      {selected && (
        <section aria-label="Revision history">
          <h2>{selected}</h2>
          <nav className="specs-session-links" aria-label="Bound sessions">
            {(specs.find((item) => item.id === selected)?.sessions ?? '').split(/\s+/).filter(Boolean).map((id) => (
              <span key={id} className="specs-session">
                <span>{id}</span>
                <a
                  href={`/logging?session=${encodeURIComponent(id)}`}
                  onClick={(event) => {
                    event.preventDefault();
                    navigateRoute({ page: 'logging', session: id });
                  }}
                >
                  Logging
                </a>
                <a
                  href={`/memory?q=${encodeURIComponent(id)}`}
                  onClick={(event) => {
                    event.preventDefault();
                    navigateRoute({ page: 'memory', recallQuery: id });
                  }}
                >
                  Memory
                </a>
              </span>
            ))}
          </nav>
          {revisions.length === 0 ? (
            <p role="status">No revisions for this spec.</p>
          ) : (
            <ItemList className="calm-list" listId="spec-revisions" columns={[{ label: 'Version' }, { label: 'Source' }, { label: 'Status' }]}>
              {revisions.map((row) => (
                <ItemRow
                  key={row.version}
                  variant="graph"
                  title={`v${row.version}`}
                  subtitle={row.source}
                  meta={row.status}
                  selected={openVersion === row.version}
                  listValues={[`v${row.version}`, row.source, row.status]}
                  onClick={() => selectVersion(row.version)}
                />
              ))}
            </ItemList>
          )}
          {current && (
            <article className="specs-revision">
              <div className="specs-history-nav">
                <button type="button" className="btn btn-compact" disabled={currentIndex <= 0} onClick={() => previous && selectVersion(previous.version)}>
                  Previous
                </button>
                <button
                  type="button"
                  className="btn btn-compact"
                  disabled={currentIndex < 0 || currentIndex >= revisions.length - 1}
                  onClick={() => selectVersion(revisions[currentIndex + 1].version)}
                >
                  Next
                </button>
                <p>
                  v{current.version} · {current.status} · {current.source}
                  {current.basedOn != null ? ` · based on v${current.basedOn}` : ''}
                  {current.createdAt > 0 ? ` · ${new Date(current.createdAt).toLocaleString()}` : ''}
                </p>
              </div>
              <p>Hash {current.hash}</p>
              {diff && (diff.added.length > 0 || diff.removed.length > 0) && (
                <div className="specs-diff" aria-label="Changes from the previous version">
                  {diff.removed.map((line, index) => (
                    <p key={`removed-${index}`} className="specs-diff-removed">- {line}</p>
                  ))}
                  {diff.added.map((line, index) => (
                    <p key={`added-${index}`} className="specs-diff-added">+ {line}</p>
                  ))}
                </div>
              )}
              <label className="specs-editor-label">
                Body
                <textarea
                  className="specs-editor"
                  aria-label="Spec body"
                  value={canEdit ? draftText : current.body}
                  readOnly={!canEdit}
                  onChange={(event) => {
                    setDraftText(event.target.value);
                    setDirty(event.target.value !== current.body);
                  }}
                />
              </label>
              <div className="specs-actions">
                {current.status !== 'draft' && !revising && (
                  <button type="button" className="btn btn-compact" onClick={() => { setRevising(true); setDraftText(current.body); setDirty(false); }}>
                    Revise
                  </button>
                )}
                {canEdit && (
                  <button type="button" className="btn btn-compact" disabled={!dirty} onClick={() => void save()}>
                    Save
                  </button>
                )}
                {dirty && (
                  <button
                    type="button"
                    className="btn btn-compact"
                    onClick={() => {
                      setDraftText(current.body);
                      setDirty(false);
                      setRevising(current.status === 'draft');
                    }}
                  >
                    Discard
                  </button>
                )}
                {current.status === 'draft' && !dirty && (
                  <button type="button" className="btn btn-compact" onClick={() => void approve(current.version)}>
                    Approve v{current.version}
                  </button>
                )}
              </div>
            </article>
          )}
        </section>
      )}
    </section>
  );
}
