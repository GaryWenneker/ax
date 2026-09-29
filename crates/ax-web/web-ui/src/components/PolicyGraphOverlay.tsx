import { useEffect, useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import ForceGraphCanvas, { type CanvasNode } from './ForceGraphCanvas';
import { ResizableBlade } from './BladeResize';
import { Spinner } from './ui/Spinner';
import PolicyRuleInlineWorkspace from './PolicyRuleInlineWorkspace';
import PolicySkillInlineWorkspace from './PolicySkillInlineWorkspace';
import {
  backlinks,
  outgoing,
  policyGraphModel,
  type PolicyGraphKind,
  type PolicyGraphModel,
  type PolicyGraphNode,
  type PolicyGraphPayload,
} from '../lib/policyGraph';
import { linkHref } from '../wikilinks';

export const POLICY_GRAPH_SETTINGS_KEY = 'ax.graph.policy.settings';

const KIND_COLOR: Record<PolicyGraphKind, string> = {
  rule: '#5b8def',
  skill: '#43b581',
  memory: '#e0a33a',
};

const KIND_LABEL: Record<PolicyGraphKind, string> = {
  rule: 'Rule',
  skill: 'Skill',
  memory: 'Memory',
};

async function fetchPolicyGraph(): Promise<PolicyGraphPayload> {
  const res = await fetch('/api/links/graph');
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json() as Promise<PolicyGraphPayload>;
}

function openItem(node: PolicyGraphNode) {
  window.history.pushState(null, '', linkHref(node));
  window.dispatchEvent(new PopStateEvent('popstate'));
}

interface Props {
  /** Key (`kind:origin:id`) of the item open on the page, preselected in the graph. */
  currentKey: string | null;
  onClose: () => void;
}

export default function PolicyGraphOverlay({ currentKey, onClose }: Props) {
  const [model, setModel] = useState<PolicyGraphModel | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(currentKey);

  const [reloadTick, setReloadTick] = useState(0);

  useEffect(() => {
    let alive = true;
    fetchPolicyGraph()
      .then((p) => alive && setModel(policyGraphModel(p)))
      .catch((e: unknown) => alive && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      alive = false;
    };
  }, [reloadTick]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      if (selected) setSelected(null);
      else onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose, selected]);

  const canvasNodes = useMemo<CanvasNode[]>(
    () =>
      (model?.nodes ?? []).map((n) => ({
        key: n.key,
        label: n.label,
        kind: n.kind,
        color: KIND_COLOR[n.kind],
        ring: n.global,
        degree: n.degree,
      })),
    [model],
  );

  const node = model?.nodes.find((n) => n.key === selected) ?? null;
  const selectedKey = node ? node.key : null;

  const go = (n: PolicyGraphNode) => {
    onClose();
    openItem(n);
  };

  const legend = (
    <>
      <div className="graph-legend-title">Legend</div>
      {(Object.keys(KIND_COLOR) as PolicyGraphKind[]).map((k) => (
        <div key={k} className="graph-legend-row">
          <span className="graph-legend-swatch" style={{ background: KIND_COLOR[k] }} />
          <span className="graph-legend-label">{KIND_LABEL[k]}</span>
        </div>
      ))}
      <div className="graph-legend-row">
        <span className="graph-legend-swatch policy-graph-ring" />
        <span className="graph-legend-label">Global</span>
      </div>
    </>
  );

  const linkList = (title: string, items: PolicyGraphNode[]) => (
    <div>
      <div className="detail-section-title">
        {title} ({items.length})
      </div>
      <div className="edge-list">
        {items.map((l) => (
          <button key={l.key} type="button" className="edge-item" onClick={() => setSelected(l.key)}>
            <span className="edge-name">{l.label}</span>
            <span className="edge-kind">{l.kind}</span>
          </button>
        ))}
      </div>
    </div>
  );

  return createPortal(
    <div className="policy-graph-overlay" role="dialog" aria-modal="true" aria-label="Policy graph">
      <div className="policy-graph-header">
        <span className="policy-graph-title">Graph · rules, skills and memories</span>
        <button type="button" className="detail-close" onClick={onClose} aria-label="Close graph">
          ×
        </button>
      </div>
      <div className="policy-graph-body">
        {error && <div className="policy-graph-empty">Could not load graph: {error}</div>}
        {!error && !model && (
          <div className="policy-graph-empty">
            <Spinner />
          </div>
        )}
        {model && model.nodes.length === 0 && <div className="policy-graph-empty">No rules, skills or memories yet.</div>}
        {model && model.nodes.length > 0 && (
          <ForceGraphCanvas
            nodes={canvasNodes}
            edges={model.edges}
            selectedKey={selectedKey}
            onSelect={setSelected}
            settingsKey={POLICY_GRAPH_SETTINGS_KEY}
            legend={legend}
          />
        )}
        {model && node && node.kind !== 'memory' && (
          <ResizableBlade>
            <aside className="detail-panel detail-panel--blade policy-graph-editor" aria-label={`Edit ${node.kind}`}>
              {node.kind === 'rule' ? (
                <PolicyRuleInlineWorkspace
                  key={node.key}
                  ruleId={node.id}
                  origin={node.origin}
                  projectId={node.projectId}
                  onClose={() => setSelected(null)}
                  onSaved={() => setReloadTick((t) => t + 1)}
                />
              ) : (
                <PolicySkillInlineWorkspace
                  key={node.key}
                  skillName={node.id}
                  origin={node.origin}
                  projectId={node.projectId}
                  onClose={() => setSelected(null)}
                  onSaved={() => setReloadTick((t) => t + 1)}
                />
              )}
            </aside>
          </ResizableBlade>
        )}
        {model && node && node.kind === 'memory' && (
          <ResizableBlade>
            <aside className="detail-panel detail-panel--blade" aria-label="Policy item">
              <div className="detail-header">
                <span className="detail-title">{node.label}</span>
                <button type="button" className="detail-close" onClick={() => setSelected(null)} aria-label="Close">
                  ×
                </button>
              </div>
              <div className="detail-body">
                <div className="detail-meta">
                  <div className="detail-kv">
                    <span className="detail-key">Kind</span>
                    <span className="detail-val">{KIND_LABEL[node.kind]}</span>
                  </div>
                  <div className="detail-kv">
                    <span className="detail-key">Scope</span>
                    <span className="detail-val">{node.origin}</span>
                  </div>
                </div>
                <button type="button" className="btn-primary" onClick={() => go(node)}>
                  Open {KIND_LABEL[node.kind].toLowerCase()}
                </button>
                {linkList('Links', outgoing(model, node.key))}
                {linkList('Backlinks', backlinks(model, node.key))}
              </div>
            </aside>
          </ResizableBlade>
        )}
      </div>
    </div>,
    document.body,
  );
}
