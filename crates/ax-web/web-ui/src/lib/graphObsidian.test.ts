import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  DEFAULT_GRAPH_SETTINGS,
  loadGraphSettings,
  saveGraphSettings,
  type StorageLike,
} from './graphSettings.ts';
import { filterGraph, groupColor, type FilterNode } from './graphFilter.ts';
import { buildNeighbors, highlightFor, PULSE_MS, selectionPulse } from './graphHover.ts';
import { forceParams, seedCluster } from './graphForces.ts';
import { LABEL_MAX_PX, labelAlpha, labelPx, placeLabels } from './graphLabels.ts';

describe('graphLabels', () => {
  it('label size: base when zoomed out, grows gently, never above the cap', () => {
    assert.equal(labelPx(0.3), 11);
    assert.equal(labelPx(1), 11);
    assert.ok(labelPx(2) > 11 && labelPx(2) < 14);
    assert.ok(labelPx(3) > labelPx(2));
    assert.equal(labelPx(10), LABEL_MAX_PX);
    assert.equal(LABEL_MAX_PX, 15);
  });
  const box = (x: number, y: number, priority: number) => ({ x, y, w: 40, h: 10, priority });
  it('drops a label that overlaps a higher-priority one', () => {
    const kept = placeLabels([box(0, 0, 1), box(10, 2, 5), box(100, 0, 0)]);
    assert.deepEqual([...kept].sort(), [1, 2]);
  });
  it('keeps labels that only touch or are apart', () => {
    assert.equal(placeLabels([box(0, 0, 1), box(40, 0, 1), box(0, 10, 1)]).size, 3);
  });
  it('equal priority: first one wins', () => {
    assert.deepEqual([...placeLabels([box(0, 0, 1), box(5, 0, 1)])], [0]);
  });
});

function memStorage(initial: Record<string, string> = {}): StorageLike {
  const data = { ...initial };
  return {
    getItem: (k) => (k in data ? data[k] : null),
    setItem: (k, v) => {
      data[k] = v;
    },
  };
}

describe('graphSettings', () => {
  it('has Obsidian defaults', () => {
    assert.equal(DEFAULT_GRAPH_SETTINGS.centerForce, 0.52);
    assert.equal(DEFAULT_GRAPH_SETTINGS.repelForce, 10);
    assert.equal(DEFAULT_GRAPH_SETTINGS.linkForce, 1);
    assert.equal(DEFAULT_GRAPH_SETTINGS.linkDistance, 250);
    assert.equal(DEFAULT_GRAPH_SETTINGS.textFadeThreshold, 0);
    assert.equal(DEFAULT_GRAPH_SETTINGS.nodeSize, 1);
    assert.equal(DEFAULT_GRAPH_SETTINGS.linkThickness, 1);
    assert.equal(DEFAULT_GRAPH_SETTINGS.arrows, false);
    assert.equal(DEFAULT_GRAPH_SETTINGS.orphans, true);
  });

  it('falls back to defaults on missing or corrupt storage', () => {
    assert.deepEqual(loadGraphSettings(memStorage()), DEFAULT_GRAPH_SETTINGS);
    assert.deepEqual(loadGraphSettings(memStorage({ 'ax.graph.settings': '{nope' })), DEFAULT_GRAPH_SETTINGS);
  });

  it('merges partial storage over defaults and round-trips', () => {
    const s = memStorage({ 'ax.graph.settings': JSON.stringify({ repelForce: 3 }) });
    const loaded = loadGraphSettings(s);
    assert.equal(loaded.repelForce, 3);
    assert.equal(loaded.linkDistance, 250);
    const next = { ...loaded, arrows: true, groups: [{ query: 'web', color: '#ff0000' }] };
    saveGraphSettings(next, s);
    assert.deepEqual(loadGraphSettings(s), next);
  });
});

const nodes: FilterNode[] = [
  { name: 'Alpha', kind: 'function', file_path: 'src/a.rs' },
  { name: 'Beta', kind: 'doc', file_path: 'docs/b.md' },
  { name: 'Gamma', kind: 'function', file_path: '' },
  { name: 'Lonely', kind: 'function', file_path: 'src/l.rs' },
];
const edges = [
  { source: 0, target: 1 },
  { source: 1, target: 2 },
];
const base = { search: '', orphans: true, attachments: true, existingOnly: false };

describe('graphFilter', () => {
  it('keeps everything with permissive filters', () => {
    const r = filterGraph(nodes, edges, base);
    assert.deepEqual([...r.nodes], [0, 1, 2, 3]);
    assert.deepEqual(r.edges, [0, 1]);
  });

  it('search matches name or path case-insensitively and prunes edges', () => {
    const r = filterGraph(nodes, edges, { ...base, search: 'SRC/' });
    assert.deepEqual([...r.nodes], [0, 3]);
    assert.deepEqual(r.edges, []);
  });

  it('orphans=false drops unlinked nodes', () => {
    assert.deepEqual([...filterGraph(nodes, edges, { ...base, orphans: false }).nodes], [0, 1, 2]);
  });

  it('attachments=false drops doc nodes and their edges', () => {
    const r = filterGraph(nodes, edges, { ...base, attachments: false });
    assert.deepEqual([...r.nodes], [0, 2, 3]);
    assert.deepEqual(r.edges, []);
  });

  it('existingOnly drops nodes without a file', () => {
    const r = filterGraph(nodes, edges, { ...base, existingOnly: true });
    assert.deepEqual([...r.nodes], [0, 1, 3]);
    assert.deepEqual(r.edges, [0]);
  });

  it('groupColor: first match wins, empty query never matches', () => {
    const groups = [
      { query: '', color: '#000000' },
      { query: 'docs', color: '#111111' },
      { query: 'b', color: '#222222' },
    ];
    assert.equal(groupColor(nodes[1], groups), '#111111');
    assert.equal(groupColor(nodes[0], groups), null);
  });
});

describe('graphHover', () => {
  const nb = buildNeighbors(4, edges);
  it('highlights node, neighbors, and incident edges', () => {
    const h = highlightFor(1, nb);
    assert.deepEqual([...h.nodes].sort(), [0, 1, 2]);
    assert.deepEqual([...h.edges].sort(), [0, 1]);
    const leaf = highlightFor(0, nb);
    assert.deepEqual([...leaf.nodes].sort(), [0, 1]);
    assert.deepEqual([...leaf.edges], [0]);
  });
  it('null hover is empty', () => {
    const h = highlightFor(null, nb);
    assert.equal(h.nodes.size, 0);
    assert.equal(h.edges.size, 0);
  });
});

describe('selectionPulse', () => {
  it('ring grows from the node outward and fades, then repeats', () => {
    const start = selectionPulse(0);
    const mid = selectionPulse(PULSE_MS / 2);
    const late = selectionPulse(PULSE_MS * 0.95);
    assert.equal(start.grow, 0);
    assert.ok(mid.grow > start.grow && late.grow > mid.grow);
    assert.ok(late.grow <= 1);
    assert.ok(start.alpha > mid.alpha && mid.alpha > late.alpha);
    assert.ok(late.alpha >= 0);
    assert.deepEqual(selectionPulse(PULSE_MS + 100), selectionPulse(100));
  });
});

describe('graphForces', () => {
  it('seeds every node inside the start cluster, deterministically', () => {
    const pts = seedCluster(500, 100, 50, 40);
    assert.equal(pts.length, 500);
    for (const p of pts) assert.ok(Math.hypot(p.x - 100, p.y - 50) <= 40 + 1e-9);
    assert.deepEqual(seedCluster(500, 100, 50, 40), pts);
    const spread = Math.max(...pts.map((p) => Math.hypot(p.x - 100, p.y - 50)));
    assert.ok(spread > 30, 'cluster uses its radius');
  });

  it('maps sliders to force parameters', () => {
    const a = forceParams(DEFAULT_GRAPH_SETTINGS);
    const b = forceParams({ ...DEFAULT_GRAPH_SETTINGS, repelForce: 20 });
    assert.ok(a.charge < 0);
    assert.ok(b.charge < a.charge);
    assert.equal(a.linkDistance, 250);
    assert.equal(a.linkStrength, 1);
    assert.ok(a.center > 0);
    assert.equal(forceParams({ ...DEFAULT_GRAPH_SETTINGS, centerForce: 0 }).center, 0);
  });
});

describe('graph settings storage key', () => {
  it('a custom key keeps its own settings apart from the default key', () => {
    const store = memStorage();
    saveGraphSettings({ ...DEFAULT_GRAPH_SETTINGS, repelForce: 3 }, store, 'ax.graph.policy.settings');
    assert.equal(loadGraphSettings(store, 'ax.graph.policy.settings').repelForce, 3);
    assert.equal(loadGraphSettings(store).repelForce, DEFAULT_GRAPH_SETTINGS.repelForce);
  });
});

describe('labelAlpha', () => {
  it('is hidden when zoomed out and fully shown past the threshold', () => {
    assert.equal(labelAlpha(1, 0), 0);
    assert.equal(labelAlpha(2.4, 0), 1);
    assert.ok(labelAlpha(2, 0) > 0 && labelAlpha(2, 0) < 1);
    assert.equal(labelAlpha(2.4, 2), 0);
  });
});
