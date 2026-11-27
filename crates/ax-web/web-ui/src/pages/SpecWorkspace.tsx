import { useEffect, useMemo, useState, type ReactNode } from 'react';
import Codicon from '../components/Codicon';
import ManifestAuthorityPanel from '../components/ManifestAuthorityPanel';
import MarkdownPreview from '../components/MarkdownPreview';
import ModalShell from '../components/ModalShell';
import { ItemList } from '../components/ui/ItemList';
import { ItemGroupHeader, ItemRow } from '../components/ui/PageLayout';
import { navigateRoute } from '../lib/routes';
import { formatSpecDate } from '../lib/specSort';
import {
  checklistLineCount,
  expandOnOpen,
  filterWorkspace,
  type Workspace,
  type WorkspaceDoc,
  type WorkspaceSpec,
  type WorkspaceStatusFilter,
} from '../lib/specWorkspace';
import type { SpecRevision } from './SpecsDetail';

type DocTab = 'overview' | 'content' | 'plans' | 'history' | 'validation' | 'dependencies';
type EvidenceTab = 'activity' | 'sessions' | 'turns' | 'memory' | 'logging' | 'decisions' | 'policy' | 'manifest';
type MobilePane = 'explorer' | 'document' | 'context';

const DOC_TABS: { id: DocTab; label: string; specOnly?: boolean }[] = [
  { id: 'overview', label: 'Overview' },
  { id: 'content', label: 'Content' },
  { id: 'plans', label: 'Plans', specOnly: true },
  { id: 'history', label: 'History' },
  { id: 'validation', label: 'Validation' },
  { id: 'dependencies', label: 'Dependencies' },
];

const EVIDENCE_TABS: { id: EvidenceTab; label: string }[] = [
  { id: 'activity', label: 'Activity' },
  { id: 'sessions', label: 'Sessions' },
  { id: 'turns', label: 'Turns' },
  { id: 'memory', label: 'Memory' },
  { id: 'logging', label: 'Logging' },
  { id: 'decisions', label: 'Decisions' },
  { id: 'policy', label: 'Rules & Skills' },
  { id: 'manifest', label: 'Context Manifest' },
];

const STATUS_FILTERS: { id: WorkspaceStatusFilter; label: string }[] = [
  { id: 'all', label: 'Any status' },
  { id: 'draft', label: 'Draft' },
  { id: 'approved', label: 'Approved' },
  { id: 'change_requested', label: 'Change requested' },
];

const MOBILE_PANES: { id: MobilePane; label: string; icon: string }[] = [
  { id: 'explorer', label: 'Explorer', icon: 'list-tree' },
  { id: 'document', label: 'Document', icon: 'book' },
  { id: 'context', label: 'Context', icon: 'references' },
];

const STATUS_ICON: Record<string, string> = {
  approved: 'check',
  change_requested: 'warning',
  draft: 'edit',
  superseded: 'history',
};

/** The widths at which styles/25-specs.css turns each side panel into a
 *  drawer; specWorkspacePage.test.ts keeps the two in step. */
const EXPLORER_DRAWER_QUERY = '(max-width: 899px)';
const CONTEXT_DRAWER_QUERY = '(max-width: 1280px)';

function statusLabel(status: string): string {
  return status === 'unknown' ? 'Status not loaded' : status.replace(/_/g, ' ');
}

function Badge({ icon, children }: { icon: string; children: ReactNode }) {
  return (
    <span className="page-item-badge">
      <Codicon name={icon} className="badge-icon" />
      {children}
    </span>
  );
}

function StatusBadge({ doc }: { doc: WorkspaceDoc }) {
  return <Badge icon={STATUS_ICON[doc.status] ?? 'question'}>{statusLabel(doc.status)}</Badge>;
}

function RelationBadge({ doc }: { doc: WorkspaceDoc }) {
  if (doc.relation === 'phase') return <Badge icon="list-ordered">Phase</Badge>;
  if (doc.relation === 'alternative') return <Badge icon="git-compare">Alternative</Badge>;
  return <Badge icon="info">Unclassified</Badge>;
}

function NotIndexed({ what }: { what: string }) {
  return (
    <p className="specws-missing" role="status">
      <Codicon name="info" />
      <span>
        Not indexed yet: {what} are not linked to documents. An empty list here does not mean there is no history.
      </span>
    </p>
  );
}

export function NewPlanModal({
  title,
  body,
  onTitle,
  onBody,
  onCreate,
  onClose,
}: {
  title: string;
  body: string;
  onTitle: (value: string) => void;
  onBody: (value: string) => void;
  onCreate: () => Promise<boolean>;
  onClose: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const ready = title.trim().length > 0 && body.trim().length > 0 && !busy;
  async function submit() {
    if (!ready) return;
    setBusy(true);
    const created = await onCreate();
    setBusy(false);
    if (created) onClose();
  }
  return (
    <ModalShell
      title="New plan"
      subtitle="The plan is stored as a draft under this spec. Nothing is approved automatically."
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn btn-compact" onClick={onClose}>Cancel</button>
          <button type="button" className="btn btn-compact btn-primary" disabled={!ready} onClick={() => void submit()}>
            Create plan
          </button>
        </>
      }
    >
      <form
        className="specws-form"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <label>
          Title
          <input value={title} onChange={(event) => onTitle(event.target.value)} aria-label="Plan title" />
        </label>
        <label>
          Body
          <textarea value={body} onChange={(event) => onBody(event.target.value)} aria-label="Plan body" rows={8} />
        </label>
      </form>
    </ModalShell>
  );
}

type Props = {
  workspace: Workspace;
  openDoc: WorkspaceDoc | null;
  revisions: SpecRevision[];
  openVersion: number | null;
  editor: ReactNode;
  canCreatePlan: boolean;
  newPlanTitle: string;
  newPlanBody: string;
  onNewPlanTitle: (value: string) => void;
  onNewPlanBody: (value: string) => void;
  onCreatePlan: () => Promise<boolean>;
  onOpen: (doc: WorkspaceDoc) => void;
  onOpenVersion: (version: number) => void;
  onCompare: (version: number) => void;
};

export default function SpecWorkspace({
  workspace,
  openDoc,
  revisions,
  openVersion,
  editor,
  canCreatePlan,
  newPlanTitle,
  newPlanBody,
  onNewPlanTitle,
  onNewPlanBody,
  onCreatePlan,
  onOpen,
  onOpenVersion,
  onCompare,
}: Props) {
  const [query, setQuery] = useState('');
  const [status, setStatus] = useState<WorkspaceStatusFilter>('all');
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const [sections, setSections] = useState({ specs: true, review: true });
  const [docTab, setDocTab] = useState<DocTab>('overview');
  const [evidenceTab, setEvidenceTab] = useState<EvidenceTab>('sessions');
  const [explorerCollapsed, setExplorerCollapsed] = useState(false);
  const [contextCollapsed, setContextCollapsed] = useState(false);
  const [drawer, setDrawer] = useState<'explorer' | 'context' | null>(null);
  const [pane, setPane] = useState<MobilePane>(openDoc ? 'document' : 'explorer');
  const [newPlanOpen, setNewPlanOpen] = useState(false);

  const shown = useMemo(() => filterWorkspace(workspace, query, status), [workspace, query, status]);
  const searching = query.trim().length > 0 || status !== 'all';
  const parent: WorkspaceSpec | null = openDoc?.parentSpecId
    ? workspace.specs.find((spec) => spec.doc.id === openDoc.parentSpecId) ?? null
    : null;
  const openSpec: WorkspaceSpec | null = openDoc?.type === 'spec'
    ? workspace.specs.find((spec) => spec.doc.id === openDoc.id) ?? null
    : null;
  const tabs = DOC_TABS.filter((tab) => !tab.specOnly || openDoc?.type === 'spec');
  const approvedBody = revisions.find((row) => row.status === 'approved')?.body;
  const readBody = approvedBody ?? revisions[revisions.length - 1]?.body ?? '';
  const checkboxes = checklistLineCount(readBody);

  useEffect(() => {
    if (openDoc) setExpanded((current) => expandOnOpen(current, openDoc));
  }, [openDoc]);

  useEffect(() => {
    if (openDoc?.type !== 'spec' && docTab === 'plans') setDocTab('overview');
  }, [openDoc?.type, docTab]);

  function toggle(id: string) {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function open(doc: WorkspaceDoc) {
    onOpen(doc);
    setExpanded((current) => expandOnOpen(current, doc));
    setPane('document');
    setDrawer(null);
  }

  function togglePanel(panel: 'explorer' | 'context') {
    const drawerWidth = panel === 'explorer' ? EXPLORER_DRAWER_QUERY : CONTEXT_DRAWER_QUERY;
    if (window.matchMedia(drawerWidth).matches) {
      setDrawer((current) => (current === panel ? null : panel));
      return;
    }
    if (panel === 'explorer') setExplorerCollapsed((value) => !value);
    else setContextCollapsed((value) => !value);
  }

  function specRow(spec: WorkspaceSpec) {
    const isOpen = searching || expanded.has(spec.doc.id);
    const rows = [
      <ItemRow
        key={spec.doc.id}
        variant="graph"
        className="specws-spec-row"
        title={spec.doc.title}
        subtitle={spec.doc.title === spec.doc.id ? undefined : spec.doc.id}
        badges={
          <>
            <StatusBadge doc={spec.doc} />
            <Badge icon="note">{spec.plans.length} {spec.plans.length === 1 ? 'plan' : 'plans'}</Badge>
          </>
        }
        selected={openDoc?.id === spec.doc.id}
        leading={
          spec.plans.length > 0 ? (
            <button
              type="button"
              className="specws-disclosure"
              aria-expanded={isOpen}
              aria-label={`${isOpen ? 'Collapse' : 'Expand'} ${spec.doc.title}`}
              onClick={(event) => {
                event.stopPropagation();
                toggle(spec.doc.id);
              }}
            >
              <Codicon name={isOpen ? 'chevron-down' : 'chevron-right'} />
            </button>
          ) : (
            <span className="specws-disclosure specws-disclosure--empty" aria-hidden="true" />
          )
        }
        onClick={() => open(spec.doc)}
      />,
    ];
    if (isOpen) {
      for (const plan of spec.plans) {
        rows.push(
          <ItemRow
            key={`${spec.doc.id}/${plan.id}`}
            variant="graph"
            className="specws-plan-row"
            title={plan.title}
            subtitle={plan.latestVersion != null ? `v${plan.latestVersion}` : undefined}
            badges={
              <>
                <RelationBadge doc={plan} />
                <StatusBadge doc={plan} />
              </>
            }
            selected={openDoc?.id === plan.id}
            onClick={() => open(plan)}
          />,
        );
      }
    }
    return rows;
  }

  const explorer = (
    <section className="specws-explorer" aria-label="Specification explorer">
      <div className="specws-pane-head">
        <label className="specws-search">
          <Codicon name="search" />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search title, id, or path"
            aria-label="Search specs and plans"
          />
        </label>
        <div className="specws-chips" role="group" aria-label="Status">
          {STATUS_FILTERS.map((filter) => (
            <button
              key={filter.id}
              type="button"
              className="btn btn-compact"
              aria-pressed={status === filter.id}
              onClick={() => setStatus(filter.id)}
            >
              {filter.label}
            </button>
          ))}
        </div>
      </div>
      <div className="specws-pane-scroll">
        <ItemList rows className="calm-list specws-tree">
          <ItemGroupHeader
            label="Specifications"
            count={shown.specs.length}
            open={sections.specs}
            onToggle={() => setSections((current) => ({ ...current, specs: !current.specs }))}
          />
          {sections.specs && shown.specs.flatMap(specRow)}
          {sections.specs && shown.specs.length === 0 && (
            <p className="specws-empty" role="status">
              {searching
                ? 'No specification matches the search or status.'
                : 'No specifications yet. Documents whose type or parent is uncertain wait under Needs review.'}
            </p>
          )}
          <ItemGroupHeader
            label="Needs review"
            count={shown.needsReview.length}
            open={sections.review}
            onToggle={() => setSections((current) => ({ ...current, review: !current.review }))}
          />
          {sections.review &&
            shown.needsReview.map((doc) => (
              <ItemRow
                key={doc.id}
                variant="graph"
                className="specws-review-row"
                title={doc.title}
                badges={
                  <>
                    <Badge icon={doc.type === 'plan' ? 'note' : 'book'}>{doc.type === 'plan' ? 'Plan' : 'Spec'}</Badge>
                    <StatusBadge doc={doc} />
                  </>
                }
                selected={openDoc?.id === doc.id}
                onClick={() => open(doc)}
              />
            ))}
          {sections.review && shown.needsReview.length === 0 && (
            <p className="specws-empty" role="status">Nothing waits for review.</p>
          )}
        </ItemList>
      </div>
    </section>
  );

  const documentPane = (
    <section className="specws-document" aria-label="Document">
      {!openDoc ? (
        <div className="specws-placeholder">
          <Codicon name="book" />
          <h2>Open a specification or plan</h2>
          <p>
            {workspace.specs.length} specifications, {workspace.specs.reduce((sum, spec) => sum + spec.plans.length, 0)} plans,{' '}
            {workspace.needsReview.length} waiting in Needs review.
          </p>
        </div>
      ) : (
        <>
          <header className="specws-doc-header">
            <nav className="specws-breadcrumb" aria-label="Breadcrumb">
              <span>Specs &amp; Plans</span>
              {parent && (
                <>
                  <Codicon name="chevron-right" />
                  <button type="button" className="specws-link" onClick={() => open(parent.doc)}>{parent.doc.title}</button>
                </>
              )}
              {!parent && openDoc.type === 'plan' && (
                <>
                  <Codicon name="chevron-right" />
                  <span>Needs review</span>
                </>
              )}
              <Codicon name="chevron-right" />
              <span aria-current="page">{openDoc.title}</span>
            </nav>
            <h2 className="specws-doc-title">
              <Codicon name={openDoc.type === 'plan' ? 'note' : 'book'} />
              <span>{openDoc.title}</span>
            </h2>
            <div className="specws-doc-meta">
              <Badge icon={openDoc.type === 'plan' ? 'note' : 'book'}>{openDoc.type === 'plan' ? 'Plan' : 'Specification'}</Badge>
              <StatusBadge doc={openDoc} />
              {openDoc.latestVersion != null && <Badge icon="tag">v{openDoc.latestVersion}</Badge>}
              {openDoc.approvedVersion != null && <Badge icon="check">approved v{openDoc.approvedVersion}</Badge>}
              {openDoc.source && <Badge icon="git-commit">{openDoc.source}</Badge>}
              {openDoc.type === 'plan' && <RelationBadge doc={openDoc} />}
            </div>
            <dl className="specws-doc-ids">
              <div><dt>ID</dt><dd>{openDoc.id}</dd></div>
              <div><dt>Owner</dt><dd>Not recorded</dd></div>
              <div><dt>Path</dt><dd>{openDoc.path || 'No file'}</dd></div>
            </dl>
          </header>
          <div className="specws-tabs" role="tablist" aria-label="Document sections">
            {tabs.map((tab) => (
              <button
                key={tab.id}
                type="button"
                role="tab"
                id={`specws-tab-${tab.id}`}
                aria-selected={docTab === tab.id}
                aria-controls="specws-doc-panel"
                className="specws-tab"
                onClick={() => setDocTab(tab.id)}
              >
                {tab.label}
              </button>
            ))}
          </div>
          <div id="specws-doc-panel" role="tabpanel" aria-labelledby={`specws-tab-${docTab}`} className={`specws-tabpanel specws-tabpanel--${docTab}`}>
            {docTab === 'overview' && (
              <div className="specws-overview">
                <dl className="specws-facts">
                  <div><dt>Status</dt><dd>{statusLabel(openDoc.status)}</dd></div>
                  <div><dt>Latest version</dt><dd>{openDoc.latestVersion != null ? `v${openDoc.latestVersion}` : '—'}</dd></div>
                  <div><dt>Approved version</dt><dd>{openDoc.approvedVersion != null ? `v${openDoc.approvedVersion}` : 'None yet'}</dd></div>
                  <div><dt>Revisions</dt><dd>{openDoc.revisionCount ?? revisions.length}</dd></div>
                  <div><dt>Updated</dt><dd>{openDoc.updatedAt ? formatSpecDate(openDoc.updatedAt) : '—'}</dd></div>
                  <div><dt>Checklist</dt><dd>{checkboxes} Markdown checkboxes, not tracked yet</dd></div>
                </dl>
                {openSpec && (
                  <section className="specws-overview-plans" aria-label="Plans of this specification">
                    <h3>Plans ({openSpec.plans.length})</h3>
                    {openSpec.plans.length === 0 ? (
                      <p role="status">No plans under this specification yet.</p>
                    ) : (
                      <ItemList rows className="calm-list specws-tree">
                        {openSpec.plans.map((plan) => (
                          <ItemRow
                            key={plan.id}
                            variant="graph"
                            title={plan.title}
                            subtitle={plan.latestVersion != null ? `v${plan.latestVersion}` : undefined}
                            badges={<><RelationBadge doc={plan} /><StatusBadge doc={plan} /></>}
                            onClick={() => open(plan)}
                          />
                        ))}
                      </ItemList>
                    )}
                  </section>
                )}
                <h3>{approvedBody != null ? `Approved text (v${openDoc.approvedVersion ?? ''})` : 'Latest text, not approved yet'}</h3>
                {readBody ? <div className="specws-read"><MarkdownPreview value={readBody} /></div> : <p role="status">Loading the text…</p>}
              </div>
            )}
            {docTab === 'content' && (
              <div className="specws-editor">
                {editor}
                {!editor && <p role="status">Loading the editor…</p>}
              </div>
            )}
            {docTab === 'plans' && openSpec && (
              <div className="specws-plans">
                <div className="specws-plans-head">
                  <p>
                    Plans whose parent is stored. Phase or alternative is shown once the relation is stored; until then a plan is Unclassified.
                  </p>
                  <button
                    type="button"
                    className="btn btn-compact"
                    disabled={!canCreatePlan}
                    title={canCreatePlan ? undefined : 'New plans need the spec hierarchy turned on for this project.'}
                    onClick={() => setNewPlanOpen(true)}
                  >
                    <Codicon name="add" /> New plan
                  </button>
                </div>
                {openSpec.plans.length === 0 ? (
                  <p role="status">No plans under this specification yet.</p>
                ) : (
                  <ItemList rows className="calm-list specws-tree">
                    {openSpec.plans.map((plan) => (
                      <ItemRow
                        key={plan.id}
                        variant="graph"
                        title={plan.title}
                        subtitle={plan.latestVersion != null ? `v${plan.latestVersion}` : undefined}
                        badges={<><RelationBadge doc={plan} /><StatusBadge doc={plan} /></>}
                        onClick={() => open(plan)}
                      />
                    ))}
                  </ItemList>
                )}
              </div>
            )}
            {docTab === 'history' && (
              revisions.length === 0 ? (
                <p role="status">Loading the history…</p>
              ) : (
                <ItemList rows className="calm-list specws-tree">
                  {[...revisions].reverse().map((row) => (
                    <ItemRow
                      key={row.version}
                      variant="graph"
                      title={`v${row.version}`}
                      subtitle={`${statusLabel(row.status)} · ${row.source}${row.basedOn != null ? ` · based on v${row.basedOn}` : ''} · ${formatSpecDate(row.createdAt)}`}
                      selected={row.version === openVersion}
                      aside={
                        <span className="specws-row-actions">
                          <button type="button" className="btn btn-compact" onClick={(event) => { event.stopPropagation(); onOpenVersion(row.version); setDocTab('content'); }}>
                            Open
                          </button>
                          <button
                            type="button"
                            className="btn btn-compact"
                            disabled={openVersion == null || row.version >= openVersion}
                            onClick={(event) => { event.stopPropagation(); onCompare(row.version); setDocTab('content'); }}
                          >
                            Compare
                          </button>
                        </span>
                      }
                      onClick={() => { onOpenVersion(row.version); setDocTab('content'); }}
                    />
                  ))}
                </ItemList>
              )
            )}
            {docTab === 'validation' && (
              <div className="specws-validation">
                <p className="specws-missing" role="status">
                  <Codicon name="info" />
                  <span>
                    {checkboxes} Markdown checkboxes in this text, not tracked yet. A checked box is not evidence until the checklist store records who checked it, when, and with what proof.
                  </span>
                </p>
                <h3>Context manifests for this project</h3>
                <p className="specws-note">Manifests are not yet tied to single documents, so this list covers the whole project.</p>
                <ManifestAuthorityPanel />
              </div>
            )}
            {docTab === 'dependencies' && (
              <div className="specws-dependencies">
                {openDoc.type === 'spec' && openSpec && openSpec.plans.length > 0 && (
                  <ItemList rows className="calm-list specws-tree">
                    {openSpec.plans.map((plan) => (
                      <ItemRow key={plan.id} variant="graph" title={plan.title} subtitle="Plan under this specification" badges={<RelationBadge doc={plan} />} onClick={() => open(plan)} />
                    ))}
                  </ItemList>
                )}
                {openDoc.type === 'spec' && (!openSpec || openSpec.plans.length === 0) && (
                  <p role="status">No stored links from this specification.</p>
                )}
                {openDoc.type === 'plan' && parent && (
                  <ItemList rows className="calm-list specws-tree">
                    <ItemRow variant="graph" title={parent.doc.title} subtitle="Specification this plan is stored under" badges={<RelationBadge doc={openDoc} />} onClick={() => open(parent.doc)} />
                  </ItemList>
                )}
                {openDoc.type === 'plan' && !parent && (
                  <p className="specws-missing" role="status">
                    <Codicon name="info" />
                    <span>
                      No stored parent. This plan stays in Needs review until someone confirms a parent, reclassifies it, or leaves it unlinked. No parent is guessed from the title.
                    </span>
                  </p>
                )}
              </div>
            )}
          </div>
        </>
      )}
    </section>
  );

  const sessions = openDoc?.sessions ?? [];
  const counts: Record<EvidenceTab, string> = {
    activity: String(revisions.length),
    sessions: String(sessions.length),
    turns: '—',
    memory: String(sessions.length),
    logging: String(sessions.length),
    decisions: '—',
    policy: '—',
    manifest: '—',
  };

  function sessionList(target: 'logging' | 'memory' | 'both') {
    if (sessions.length === 0) {
      return <p role="status">No session is bound to this document.</p>;
    }
    return (
      <ul className="specws-sessions">
        {sessions.map((session) => (
          <li key={session}>
            <code title={session}>{session}</code>
            <span className="specws-row-actions">
              {target !== 'memory' && (
                <button type="button" className="btn btn-compact" onClick={() => navigateRoute({ page: 'logging', session })}>Logging</button>
              )}
              {target !== 'logging' && (
                <button type="button" className="btn btn-compact" onClick={() => navigateRoute({ page: 'memory', session })}>Memory</button>
              )}
            </span>
          </li>
        ))}
      </ul>
    );
  }

  const contextPane = (
    <section className="specws-context" aria-label="Context and evidence">
      <div className="specws-pane-head">
        <h2 className="specws-pane-title">Context &amp; evidence</h2>
        <p className="specws-note">{openDoc ? `Scope: ${openDoc.title}` : 'Open a document to see its context.'}</p>
      </div>
      <div className="specws-evidence-tabs" role="tablist" aria-label="Evidence">
        {EVIDENCE_TABS.map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={evidenceTab === tab.id}
            className="specws-tab"
            onClick={() => setEvidenceTab(tab.id)}
          >
            {tab.label}
            <span className="specws-count">{openDoc ? counts[tab.id] : '—'}</span>
          </button>
        ))}
      </div>
      <div className="specws-pane-scroll specws-evidence" role="tabpanel">
        {!openDoc ? (
          <p role="status">No document is open.</p>
        ) : evidenceTab === 'activity' ? (
          revisions.length === 0 ? (
            <p role="status">Loading the activity…</p>
          ) : (
            <ol className="specws-activity">
              {[...revisions].reverse().map((row) => (
                <li key={row.version}>
                  <strong>v{row.version}</strong> {statusLabel(row.status)} · {row.source}
                  <time>{formatSpecDate(row.createdAt)}</time>
                </li>
              ))}
            </ol>
          )
        ) : evidenceTab === 'sessions' ? (
          sessionList('both')
        ) : evidenceTab === 'logging' ? (
          <>
            <p className="specws-note">Opens Logging filtered by one session. Filtering by document is not available yet.</p>
            {sessionList('logging')}
          </>
        ) : evidenceTab === 'memory' ? (
          <>
            <p className="specws-note">Opens Memory filtered by one session. Filtering by document is not available yet.</p>
            {sessionList('memory')}
          </>
        ) : evidenceTab === 'turns' ? (
          <NotIndexed what="turns" />
        ) : evidenceTab === 'decisions' ? (
          <NotIndexed what="decisions" />
        ) : evidenceTab === 'policy' ? (
          <NotIndexed what="rule and skill revisions" />
        ) : (
          <>
            <NotIndexed what="context manifests" />
            <button type="button" className="btn btn-compact" onClick={() => { setDocTab('validation'); setPane('document'); }}>
              Show project manifests
            </button>
          </>
        )}
      </div>
    </section>
  );

  return (
    <div
      className={`specws${explorerCollapsed ? ' specws--explorer-collapsed' : ''}${contextCollapsed ? ' specws--context-collapsed' : ''}${drawer ? ` specws--drawer-${drawer}` : ''}`}
      data-pane={pane}
    >
      <div className="specws-toolbar">
        <button type="button" className="btn btn-compact" aria-label="Show or hide the explorer" onClick={() => togglePanel('explorer')}>
          <Codicon name="list-tree" /> Explorer
        </button>
        <button type="button" className="btn btn-compact" aria-label="Show or hide context and evidence" onClick={() => togglePanel('context')}>
          <Codicon name="references" /> Context
        </button>
      </div>
      {explorer}
      {documentPane}
      {contextPane}
      <nav className="specws-mobile-bar" aria-label="Workspace panes">
        {MOBILE_PANES.map((item) => (
          <button key={item.id} type="button" className="btn" aria-pressed={pane === item.id} onClick={() => setPane(item.id)}>
            <Codicon name={item.icon} />
            {item.label}
          </button>
        ))}
      </nav>
      {newPlanOpen && openSpec && (
        <NewPlanModal
          title={newPlanTitle}
          body={newPlanBody}
          onTitle={onNewPlanTitle}
          onBody={onNewPlanBody}
          onCreate={onCreatePlan}
          onClose={() => setNewPlanOpen(false)}
        />
      )}
    </div>
  );
}
