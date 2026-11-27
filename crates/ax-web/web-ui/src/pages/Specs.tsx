import { useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';
import { createPortal } from 'react-dom';
import Codicon from '../components/Codicon';
import { ItemList } from '../components/ui/ItemList';
import { ItemRow } from '../components/ui/PageLayout';
import { formatUnifiedDiff, unifiedDiff } from '../lib/unifiedDiff';
import {
  hierarchySpecRows,
  parseSpecPayload,
  plansForSpec,
  type HierarchyPlan,
  type HierarchySpec,
} from '../lib/hierarchyList';
import { formatSpecDate, sortSpecs, specVersionLabel } from '../lib/specSort';
import { linkedSessions } from '../lib/specSession';
import { navigateRoute } from '../lib/routes';
import ManifestAuthorityPanel from '../components/ManifestAuthorityPanel';
import SpecsDetail, { type EditorMode, type SpecRevision } from './SpecsDetail';
import SpecWorkspace, { NewPlanModal } from './SpecWorkspace';
import { workspaceFromHierarchy, workspaceFromLegacy, type WorkspaceDoc } from '../lib/specWorkspace';
import type { ListColumn } from '../components/ui/ListColumns';
import {
  approveSpec,
  createAlternativePlan,
  fetchPlanRevisions,
  fetchSpecList,
  fetchSpecRevisions,
  saveSpecPlanRevision,
  saveSpecRevision,
} from '../specsApi';

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
  updatedAt?: number | null;
  latestVersion?: number | null;
  latestSource?: string | null;
  latestBasedOn?: number | null;
};

type KindFilter = 'all' | 'plan' | 'spec';
type StatusFilter = 'all' | 'draft' | 'approved' | 'change_requested';
type BladeTab = 'agreement' | 'plans' | 'authority';
type EditorTarget = 'spec' | 'plan';
type SpecsView = 'workspace' | 'documents';

const VIEWS: { id: SpecsView; label: string }[] = [
  { id: 'workspace', label: 'Workspace' },
  { id: 'documents', label: 'All documents' },
];

function initialView(): SpecsView {
  if (typeof window === 'undefined') return 'workspace';
  return new URLSearchParams(window.location.search).get('view') === 'documents' ? 'documents' : 'workspace';
}

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

const BLADE_TABS: { id: BladeTab; label: string }[] = [
  { id: 'agreement', label: 'Agreement' },
  { id: 'plans', label: 'Plans' },
  { id: 'authority', label: 'Authority' },
];

const SPEC_COLUMNS: readonly ListColumn[] = [
  { label: 'Title', minWidth: 180 },
  { label: 'Path', minWidth: 72 },
  { label: 'Version', minWidth: 260 },
  { label: 'Logging', sortable: false, filterable: false, minWidth: 72 },
  { label: 'Memory', sortable: false, filterable: false, minWidth: 72 },
  { label: 'Updated', minWidth: 108 },
  { label: 'Kind', minWidth: 56 },
];

const PAGE = 20;
const WEB_SESSION = 'web-ui';
const WEB_TURN = 'local';

function stopRow(event: { stopPropagation: () => void }) {
  event.stopPropagation();
}

function openLogging(event: MouseEvent<HTMLButtonElement>, id: string) {
  event.preventDefault();
  event.stopPropagation();
  navigateRoute({ page: 'logging', session: id });
}

function openMemory(event: MouseEvent<HTMLButtonElement>, id: string) {
  event.preventDefault();
  event.stopPropagation();
  navigateRoute({ page: 'memory', session: id });
}

function shortSession(id: string): string {
  return id.length > 14 ? `${id.slice(0, 8)}…` : id;
}

function kindLabel(kind: string): string {
  return kind === 'plan' ? 'Plan' : 'Spec';
}

function statusIcon(status: string): string {
  if (status === 'approved') return 'check';
  if (status === 'change_requested') return 'warning';
  return 'edit';
}

function versionBadges(spec: SpecItem) {
  const version = spec.latestVersion;
  if (typeof version !== 'number' || version <= 0) return <span key="version" className="specs-col-version" />;
  return (
    <span key="version" className="specs-col-version">
      <span className="page-item-badge">
        <Codicon name="tag" className="badge-icon" />
        v{version}
      </span>
      {spec.latestStatus && spec.latestStatus !== 'none' && (
        <span className="page-item-badge">
          <Codicon name={statusIcon(spec.latestStatus)} className="badge-icon" />
          {spec.latestStatus.replace(/_/g, ' ')}
        </span>
      )}
      {spec.latestSource && (
        <span className="page-item-badge">
          <Codicon name="git-commit" className="badge-icon" />
          {spec.latestSource}
        </span>
      )}
      {typeof spec.latestBasedOn === 'number' && (
        <span className="page-item-badge">
          <Codicon name="git-branch" className="badge-icon" />
          v{spec.latestBasedOn}
        </span>
      )}
    </span>
  );
}

function jumpColumn(ids: string[], kind: 'logging' | 'memory') {
  if (kind === 'logging') {
    return (
      <span key="logging" className="specs-col-logging" onClick={stopRow}>
        {ids.length === 0 && <span className="specs-jump-empty">—</span>}
        {ids.map((id) => (
          <button
            key={id}
            type="button"
            className="specs-jump"
            title={id}
            aria-label={`Logging ${shortSession(id)}`}
            onClick={(event) => openLogging(event, id)}
          >
            Logging
          </button>
        ))}
      </span>
    );
  }
  return (
    <span key="memory" className="specs-col-memory" onClick={stopRow}>
      {ids.length === 0 && <span className="specs-jump-empty">—</span>}
      {ids.map((id) => (
        <button
          key={id}
          type="button"
          className="specs-jump"
          title={id}
          aria-label={`Memory ${shortSession(id)}`}
          onClick={(event) => openMemory(event, id)}
        >
          Memory
        </button>
      ))}
    </span>
  );
}

function syncRoute(specId: string | null, planId: string | null, tab: BladeTab) {
  const url = new URL(window.location.href);
  if (!specId) {
    url.searchParams.delete('spec');
    url.searchParams.delete('plan');
    url.searchParams.delete('tab');
  } else {
    url.searchParams.set('spec', specId);
    if (planId) url.searchParams.set('plan', planId);
    else url.searchParams.delete('plan');
    if (tab === 'agreement') url.searchParams.delete('tab');
    else url.searchParams.set('tab', tab);
  }
  window.history.replaceState({}, '', `${url.pathname}${url.search}${url.hash}`);
}

export default function SpecsPage() {
  const [specs, setSpecs] = useState<SpecItem[]>([]);
  const [hierarchy, setHierarchy] = useState<HierarchySpec[] | null>(null);
  const [projectId, setProjectId] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [selectedPlan, setSelectedPlan] = useState<string | null>(null);
  const [bladeTab, setBladeTab] = useState<BladeTab>('agreement');
  const [editorTarget, setEditorTarget] = useState<EditorTarget>('spec');
  const [revisions, setRevisions] = useState<SpecRevision[]>([]);
  const [openVersion, setOpenVersion] = useState<number | null>(null);
  const [baseChoice, setBaseChoice] = useState<number | null>(null);
  const [draftText, setDraftText] = useState('');
  const [dirty, setDirty] = useState(false);
  const [mode, setMode] = useState<EditorMode>('split');
  const [kindFilter, setKindFilter] = useState<KindFilter>('all');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');
  const [query, setQuery] = useState('');
  const [shown, setShown] = useState(PAGE);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [closing, setClosing] = useState(false);
  const [altTitle, setAltTitle] = useState('');
  const [altBody, setAltBody] = useState('');
  const [view, setView] = useState<SpecsView>(initialView());
  const [newPlanOpen, setNewPlanOpen] = useState(false);
  const loadToken = useRef(0);
  const closeTimer = useRef<number | null>(null);
  const pane = useRef<HTMLDivElement>(null);
  const sentinel = useRef<HTMLDivElement>(null);
  /** The open document and its unsaved state, for the popstate listener registered once. */
  const openState = useRef({ dirty, selected, selectedPlan, bladeTab });
  openState.current = { dirty, selected, selectedPlan, bladeTab };

  async function load() {
    try {
      const data = await fetchSpecList();
      setError(data.error ? String(data.error) : '');
      const parsed = parseSpecPayload(data);
      if (parsed.mode === 'hierarchy') {
        setHierarchy(parsed.nodes);
        setProjectId(parsed.projectId);
        setSpecs([]);
      } else {
        setHierarchy(null);
        setProjectId('');
        setSpecs(parsed.specs as SpecItem[]);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not load specs');
    }
  }

  useEffect(() => {
    void load();
  }, []);

  function showVersion(rows: SpecRevision[], version: number | null, keepText: boolean) {
    setOpenVersion(version);
    const row = rows.find((item) => item.version === version);
    if (!row || keepText) return;
    setDraftText(row.body);
    setDirty(false);
  }

  async function loadRevisions(id: string, version: number | null) {
    const token = ++loadToken.current;
    setSelected(id);
    setEditorTarget('spec');
    setSelectedPlan(null);
    try {
      const data = await fetchSpecRevisions(id);
      if (token !== loadToken.current) return;
      if (data.error) setError(String(data.error));
      const rows: SpecRevision[] = Array.isArray(data.revisions) ? data.revisions : [];
      setRevisions(rows);
      const approved = rows.find((row) => row.status === 'approved');
      const newest = rows.length > 0 ? rows[rows.length - 1].version : null;
      const exists = version != null && rows.some((row) => row.version === version);
      showVersion(rows, exists ? version : approved?.version ?? newest, false);
    } catch (err) {
      if (token !== loadToken.current) return;
      setError(err instanceof Error ? err.message : 'Could not load revisions');
    }
  }

  async function loadPlanRevisions(planId: string, version: number | null) {
    const token = ++loadToken.current;
    setSelectedPlan(planId);
    setEditorTarget('plan');
    setBladeTab('plans');
    try {
      const data = await fetchPlanRevisions(planId);
      if (token !== loadToken.current) return;
      if (data.error) setError(String(data.error));
      const rows: SpecRevision[] = Array.isArray(data.revisions) ? data.revisions : [];
      setRevisions(rows);
      const approved = rows.find((row) => row.status === 'approved');
      const newest = rows.length > 0 ? rows[rows.length - 1].version : null;
      const exists = version != null && rows.some((row) => row.version === version);
      showVersion(rows, exists ? version : approved?.version ?? newest, false);
    } catch (err) {
      if (token !== loadToken.current) return;
      setError(err instanceof Error ? err.message : 'Could not load plan revisions');
    }
  }

  function closeBlade() {
    if (!selected || closing) return;
    if (dirty) {
      setNotice('Discard or save unsaved changes before closing.');
      return;
    }
    setClosing(true);
    closeTimer.current = window.setTimeout(() => {
      closeTimer.current = null;
      setSelected(null);
      setSelectedPlan(null);
      setBladeTab('agreement');
      setEditorTarget('spec');
      setRevisions([]);
      setOpenVersion(null);
      setDirty(false);
      setClosing(false);
      setAltTitle('');
      setAltBody('');
      syncRoute(null, null, 'agreement');
    }, 320);
  }

  function openSpec(id: string, tab: BladeTab = 'agreement', planId: string | null = null) {
    if (dirty) {
      setNotice('Discard or save unsaved changes before opening another document.');
      return;
    }
    if (closeTimer.current != null) {
      window.clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }
    if (selected === id && !closing && tab === bladeTab && planId === selectedPlan) {
      closeBlade();
      return;
    }
    setClosing(false);
    setNotice('');
    setBaseChoice(null);
    setBladeTab(tab);
    setAltTitle('');
    setAltBody('');
    if (planId) {
      setSelected(id);
      void loadPlanRevisions(planId, null);
      syncRoute(id, planId, 'plans');
      return;
    }
    void loadRevisions(id, null).then(() => {
      syncRoute(id, null, tab);
    });
  }

  useEffect(() => {
    function openFromRoute() {
      const searchParams = new URLSearchParams(window.location.search);
      const id = searchParams.get('spec');
      if (!id) return;
      const open = openState.current;
      if (open.dirty) {
        setNotice('Discard or save unsaved changes before opening another document.');
        syncRoute(open.selected, open.selectedPlan, open.bladeTab);
        return;
      }
      const plan = searchParams.get('plan');
      const tabParam = searchParams.get('tab');
      const tab: BladeTab =
        plan ? 'plans' : tabParam === 'plans' || tabParam === 'authority' ? tabParam : 'agreement';
      setClosing(false);
      setSelected(id);
      setBladeTab(tab);
      if (plan) {
        void loadPlanRevisions(plan, null);
      } else {
        void loadRevisions(id, null);
      }
    }
    openFromRoute();
    window.addEventListener('popstate', openFromRoute);
    return () => window.removeEventListener('popstate', openFromRoute);
  }, []);

  useEffect(() => {
    if (!selected || view !== 'documents') return undefined;
    function onKey(event: KeyboardEvent) {
      if (event.key === 'Escape') closeBlade();
    }
    function onPointer(event: PointerEvent) {
      const target = event.target;
      if (!(target instanceof Element)) return;
      if (target.closest('.specs-blade, .specs-page .page-item, .ax-modal, .settings-select-menu')) return;
      closeBlade();
    }
    document.addEventListener('keydown', onKey);
    document.addEventListener('pointerdown', onPointer);
    return () => {
      document.removeEventListener('keydown', onKey);
      document.removeEventListener('pointerdown', onPointer);
    };
  });

  useEffect(() => () => {
    if (closeTimer.current != null) window.clearTimeout(closeTimer.current);
  }, []);

  function selectVersion(version: number) {
    showVersion(revisions, version, dirty);
  }

  function switchTab(tab: BladeTab) {
    if (!selected) return;
    if (dirty) {
      setNotice('Discard or save unsaved changes before switching.');
      return;
    }
    setBladeTab(tab);
    if (tab === 'agreement') {
      void loadRevisions(selected, null);
      syncRoute(selected, null, tab);
      return;
    }
    if (tab === 'plans') {
      setSelectedPlan(null);
      setEditorTarget('spec');
      setRevisions([]);
      setOpenVersion(null);
      syncRoute(selected, null, tab);
      return;
    }
    setSelectedPlan(null);
    syncRoute(selected, null, tab);
  }

  async function approve(version: number) {
    if (!selected || dirty || editorTarget !== 'spec') return;
    try {
      const data = await approveSpec(selected, version);
      if (!data.ok) {
        setError(String(data.error || 'Approval failed'));
        return;
      }
      await load();
      await loadRevisions(selected, Number(data.version));
      setNotice(`Approved version ${data.version}. Earlier revisions stay in the history.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Approval failed');
    }
  }

  async function save() {
    if (!selected || openVersion == null || !dirty) return;
    try {
      if (editorTarget === 'plan' && selectedPlan) {
        const data = await saveSpecPlanRevision({
          planId: selectedPlan,
          basedOn: openVersion,
          body: draftText,
          sessionId: WEB_SESSION,
          turnId: WEB_TURN,
        });
        if (!data.ok) {
          setError(String(data.error || 'Save failed'));
          return;
        }
        setDirty(false);
        await load();
        await loadPlanRevisions(selectedPlan, Number(data.planVersion));
        setNotice(`Saved plan draft v${data.planVersion}. The previous body is still in the history.`);
        return;
      }
      const data = await saveSpecRevision(selected, openVersion, draftText);
      if (!data.ok) {
        setError(String(data.error || 'Save failed'));
        return;
      }
      setDirty(false);
      await load();
      await loadRevisions(selected, Number(data.version));
      setNotice(`Saved draft v${data.version}. The previous body is still in the history.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Save failed');
    }
  }

  async function createAlternative(): Promise<boolean> {
    if (!selected || !projectId || !altTitle.trim() || !altBody.trim()) return false;
    const planId = `plan-${Date.now().toString(36)}`;
    try {
      const data = await createAlternativePlan({
        projectId,
        specId: selected,
        planId,
        planTitle: altTitle.trim(),
        planBody: altBody.trim(),
        sessionId: WEB_SESSION,
        turnId: WEB_TURN,
      });
      if (!data.ok) {
        setError(String(data.error || 'Could not create plan'));
        return false;
      }
      setAltTitle('');
      setAltBody('');
      await load();
      await loadPlanRevisions(String(data.planId || planId), Number(data.planVersion) || 1);
      setNotice(`Created plan ${data.planId || planId}.`);
      syncRoute(selected, String(data.planId || planId), 'plans');
      return true;
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not create plan');
      return false;
    }
  }

  function selectView(next: SpecsView) {
    if (next === view) return;
    if (dirty) {
      setNotice('Discard or save unsaved changes before switching views.');
      return;
    }
    setView(next);
    const url = new URL(window.location.href);
    if (next === 'documents') url.searchParams.set('view', 'documents');
    else url.searchParams.delete('view');
    window.history.replaceState({}, '', `${url.pathname}${url.search}${url.hash}`);
  }

  function openWorkspaceDoc(doc: WorkspaceDoc) {
    if (dirty) {
      setNotice('Discard or save unsaved changes before opening another document.');
      return;
    }
    setNotice('');
    setBaseChoice(null);
    setClosing(false);
    if (doc.type === 'plan' && doc.parentSpecId) {
      setSelected(doc.parentSpecId);
      void loadPlanRevisions(doc.id, null);
      syncRoute(doc.parentSpecId, doc.id, 'plans');
      return;
    }
    void loadRevisions(doc.id, null);
    syncRoute(doc.id, null, 'agreement');
  }

  const current = revisions.find((row) => row.version === openVersion);
  const currentIndex = revisions.findIndex((row) => row.version === openVersion);
  const older = currentIndex > 0 ? revisions.slice(0, currentIndex) : [];
  const base = older.find((row) => row.version === baseChoice) ?? older[older.length - 1] ?? null;
  const baseBody = base?.body ?? null;
  const currentBody = current?.body ?? null;
  // Typing in the filter re-renders the page; the diff only depends on the two bodies.
  const hunks = useMemo(
    () => (baseBody !== null && currentBody !== null ? unifiedDiff(baseBody, currentBody) : []),
    [baseBody, currentBody],
  );
  const diffText = useMemo(() => formatUnifiedDiff(hunks), [hunks]);
  const needle = query.trim().toLowerCase();
  const hierarchyMode = hierarchy !== null;
  const treeRows = hierarchy === null
    ? null
    : hierarchySpecRows(hierarchy).filter((row) => {
        if (!needle) return true;
        return `${row.title} ${row.specId}`.toLowerCase().includes(needle);
      });
  const openPlans: HierarchyPlan[] =
    hierarchy && selected ? plansForSpec(hierarchy, selected) : [];
  const visible = specs.filter((spec) => {
    if (kindFilter === 'plan' && spec.kind !== 'plan') return false;
    if (kindFilter === 'spec' && spec.kind !== 'spec') return false;
    if (statusFilter !== 'all' && spec.latestStatus !== statusFilter) return false;
    if (!needle) return true;
    const haystack = `${spec.title} ${spec.id} ${spec.path} ${spec.sessions}`.toLowerCase();
    return haystack.includes(needle);
  });
  const ordered = sortSpecs(visible, 'newest');
  const windowed = ordered.slice(0, shown);
  useEffect(() => {
    setShown(PAGE);
  }, [kindFilter, statusFilter, query]);
  useEffect(() => {
    const node = sentinel.current;
    const root = pane.current;
    if (!node || !root) return undefined;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) setShown((count) => count + PAGE);
    }, { root, rootMargin: '160px' });
    observer.observe(node);
    return () => observer.disconnect();
  }, [shown, ordered.length]);
  const openItem = specs.find((item) => item.id === selected);
  const bladeTitle = selectedPlan || openItem?.title || selected || 'Spec';
  const bladeHost = typeof document !== 'undefined' ? document.querySelector('.workspace') : null;

  function renderEditor(title: string) {
    if (!current) return null;
    const open = current;
    return (
      <SpecsDetail
        title={title}
        revisions={revisions}
        current={open}
        openVersion={openVersion}
        older={older}
        base={base}
        mode={mode}
        draftText={draftText}
        dirty={dirty}
        diffText={diffText}
        onSelectVersion={selectVersion}
        onBaseChoice={setBaseChoice}
        onMode={setMode}
        onDraft={(value) => {
          setDraftText(value);
          setDirty(value !== open.body);
        }}
        onSave={() => void save()}
        onDiscard={() => {
          setDraftText(open.body);
          setDirty(false);
        }}
        onApprove={editorTarget === 'spec' ? (version) => void approve(version) : undefined}
      />
    );
  }

  const editorBody = bladeTab === 'agreement' || (bladeTab === 'plans' && selectedPlan) ? renderEditor(bladeTitle) : null;
  const workspace = useMemo(
    () => (hierarchy ? workspaceFromHierarchy(hierarchy) : workspaceFromLegacy(specs)),
    [hierarchy, specs],
  );
  const openDocId = selectedPlan ?? selected;
  const openDoc = openDocId ? workspace.all.find((doc) => doc.id === openDocId) ?? null : null;

  const plansBody = bladeTab === 'plans' && !selectedPlan ? (
    <div className="specs-plans-panel">
      {openPlans.length === 0 ? (
        <p role="status">No plans under this spec yet.</p>
      ) : (
        <ItemList className="calm-list specs-plan-list" listId="specs-plans" columns={[
          { label: 'Plan', minWidth: 160 },
          { label: 'Version', minWidth: 72 },
        ]}>
          {openPlans.map((plan) => (
            <ItemRow
              key={plan.planId}
              variant="graph"
              title={plan.planId}
              selected={false}
              portalColumns={[
                <span key="version" className="specs-col-version">v{plan.latestVersion}</span>,
              ]}
              portalLabels={['Version']}
              listValues={[plan.planId, plan.latestVersion]}
              onClick={() => {
                if (!selected) return;
                void loadPlanRevisions(plan.planId, null);
                syncRoute(selected, plan.planId, 'plans');
              }}
            />
          ))}
        </ItemList>
      )}
      <button type="button" className="btn btn-compact" disabled={!projectId} onClick={() => setNewPlanOpen(true)}>
        <Codicon name="add" /> New plan
      </button>
    </div>
  ) : null;
  const newPlanModal = newPlanOpen ? (
    <NewPlanModal
      title={altTitle}
      body={altBody}
      onTitle={setAltTitle}
      onBody={setAltBody}
      onCreate={createAlternative}
      onClose={() => setNewPlanOpen(false)}
    />
  ) : null;

  const bladeContent =
    !hierarchyMode
      ? editorBody
      : bladeTab === 'authority'
        ? <ManifestAuthorityPanel />
        : bladeTab === 'plans' && !selectedPlan
          ? plansBody
          : editorBody;

  const bladeFrame = selected && bladeContent ? (
    <aside className={`memory-blade specs-blade${closing ? ' specs-blade--closing' : ''}`} role="complementary" aria-label={bladeTitle}>
      <div className="detail-header">
        <span className="detail-title memory-blade-title">
          <Codicon name={selectedPlan || openItem?.kind === 'plan' ? 'note' : 'book'} className="detail-title-icon" />
          <span>{bladeTitle}</span>
        </span>
        <button type="button" className="detail-close" aria-label="Close" onClick={closeBlade}>
          <Codicon name="close" />
        </button>
      </div>
      {hierarchyMode && (
        <div className="specs-blade-nav">
          <nav className="specs-blade-tabs" aria-label="Spec sections">
            {BLADE_TABS.map((tab) => (
              <button
                key={tab.id}
                type="button"
                className="btn btn-compact"
                aria-pressed={bladeTab === tab.id}
                onClick={() => switchTab(tab.id)}
              >
                {tab.label}
              </button>
            ))}
          </nav>
          {selectedPlan && (
            <div className="specs-blade-crumb">
              <button
                type="button"
                className="btn btn-compact"
                onClick={() => {
                  if (dirty) {
                    setNotice('Discard or save unsaved changes before leaving the plan.');
                    return;
                  }
                  setSelectedPlan(null);
                  setEditorTarget('spec');
                  setRevisions([]);
                  setOpenVersion(null);
                  setBladeTab('plans');
                  if (selected) syncRoute(selected, null, 'plans');
                }}
              >
                Spec
              </button>
              <span aria-hidden="true"> › </span>
              <span>{selectedPlan}</span>
            </div>
          )}
        </div>
      )}
      <div className="detail-body memory-blade-body">{bladeContent}</div>
    </aside>
  ) : null;
  const blade = bladeFrame && bladeHost ? createPortal(bladeFrame, bladeHost) : bladeFrame;

  const statusLines = (
    <>
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      {dirty && <p role="status">Unsaved changes</p>}
    </>
  );

  return (
    <section className={`page-stack specs-page${view === 'workspace' ? ' specs-page--ws' : ''}`}>
      <header className="page-hero specs-hero">
        <div>
          <h1>Specs &amp; Plans</h1>
          <p>
            {view === 'workspace'
              ? 'Specifications with the plans under them. An approved revision stays the agreement; an edit is saved as the next draft.'
              : hierarchyMode
                ? 'Open a spec first. Plans live under that agreement.'
                : 'Approved specs stay the agreement. An edit is saved as the next draft.'}
          </p>
        </div>
        <div className="specs-view-switch" role="group" aria-label="View">
          {VIEWS.map((item) => (
            <button
              key={item.id}
              type="button"
              className="btn btn-compact"
              aria-pressed={view === item.id}
              onClick={() => selectView(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>
      </header>
      {view === 'workspace' ? (
        <>
          {statusLines}
          <SpecWorkspace
            workspace={workspace}
            openDoc={openDoc}
            revisions={revisions}
            openVersion={openVersion}
            editor={renderEditor(openDoc?.title ?? bladeTitle)}
            canCreatePlan={Boolean(projectId)}
            newPlanTitle={altTitle}
            newPlanBody={altBody}
            onNewPlanTitle={setAltTitle}
            onNewPlanBody={setAltBody}
            onCreatePlan={createAlternative}
            onOpen={openWorkspaceDoc}
            onOpenVersion={selectVersion}
            onCompare={(version) => {
              setBaseChoice(version);
              setMode('diff');
            }}
          />
        </>
      ) : (
      <>
      {!hierarchyMode && <ManifestAuthorityPanel />}
      <div className="specs-filters">
        {!hierarchyMode && (
          <>
            <div className="specs-filter-group" role="group" aria-label="Kind">
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
            </div>
            <div className="specs-filter-group" role="group" aria-label="Status">
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
            </div>
          </>
        )}
        <label className="specs-search">
          Search
          <input
            aria-label="Search specs"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
      </div>
      {statusLines}
      <div className="specs-list-pane" ref={pane}>
          {treeRows !== null ? (
            treeRows.length === 0 ? (
              <p role="status">No specs stored yet. A finished plan file or a finished plan request appears here as a draft.</p>
            ) : (
              <ItemList className="calm-list specs-list" listId="specs" columns={SPEC_COLUMNS}>
                {treeRows.map((row) => (
                  <ItemRow
                    key={row.key}
                    variant="graph"
                    title={row.title}
                    selected={selected === row.specId}
                    portalColumns={[
                      <span key="path" className="specs-col-path" title="No file">No file</span>,
                      <span key="version" className="specs-col-version" />,
                      jumpColumn([], 'logging'),
                      jumpColumn([], 'memory'),
                      <span key="updated" className="specs-col-updated" />,
                      <span key="kind" className="page-item-badge">
                        <Codicon name="book" className="badge-icon" />
                        Spec
                      </span>,
                    ]}
                    portalLabels={['Path', 'Version', 'Logging', 'Memory', 'Updated', 'Kind']}
                    listValues={[row.title, 'No file', 0, '', '', 0, 'Spec']}
                    onClick={() => void openSpec(row.specId)}
                  />
                ))}
              </ItemList>
            )
          ) : specs.length === 0 ? (
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
            <>
            <ItemList className="calm-list specs-list" listId="specs" columns={SPEC_COLUMNS}>
              {windowed.map((spec) => {
                const dateLabel = formatSpecDate(spec.updatedAt);
                const versionLabel = specVersionLabel(spec);
                const bound = linkedSessions(spec.sessions);
                return (
                <ItemRow
                  key={spec.id}
                  variant="graph"
                  title={spec.title || spec.id}
                  selected={selected === spec.id}
                  portalColumns={[
                    <span key="path" className="specs-col-path" title={spec.path || 'No file'}>{spec.path || 'No file'}</span>,
                    versionBadges(spec),
                    jumpColumn(bound, 'logging'),
                    jumpColumn(bound, 'memory'),
                    <span key="updated" className="specs-col-updated">{dateLabel}</span>,
                    <span key="kind" className="page-item-badge">
                      <Codicon name={spec.kind === 'plan' ? 'note' : 'book'} className="badge-icon" />
                      {kindLabel(spec.kind)}
                    </span>,
                  ]}
                  portalLabels={['Path', 'Version', 'Logging', 'Memory', 'Updated', 'Kind']}
                  listValues={[
                    spec.title || spec.id,
                    spec.path || 'No file',
                    versionLabel,
                    bound.join(' '),
                    bound.join(' '),
                    spec.updatedAt ?? 0,
                    kindLabel(spec.kind),
                  ]}
                  onClick={() => void openSpec(spec.id)}
                />
                );
              })}
            </ItemList>
            {shown < ordered.length && <div ref={sentinel} className="specs-scroll-sentinel" aria-hidden="true" />}
            </>
          )}
      </div>
      {blade}
      {newPlanModal}
      </>
      )}
    </section>
  );
}
