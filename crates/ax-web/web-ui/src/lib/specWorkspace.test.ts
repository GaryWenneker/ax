import assert from 'node:assert/strict';
import test from 'node:test';
import {
  checklistLineCount,
  expandOnOpen,
  filterWorkspace,
  workspaceFromHierarchy,
  workspaceFromLegacy,
  type LegacySpecItem,
} from './specWorkspace.ts';

function legacy(id: string, kind: string, extra: Partial<LegacySpecItem> = {}): LegacySpecItem {
  return {
    id,
    title: `Title ${id}`,
    path: `docs/specs/${id}/SPEC.md`,
    kind,
    sessions: '',
    approvedVersion: null,
    latestStatus: 'draft',
    latestVersion: 1,
    latestSource: 'plan_mode',
    revisionCount: 1,
    updatedAt: 1,
    ...extra,
  };
}

test('legacy plans without a parent go to Needs review', () => {
  const ws = workspaceFromLegacy([legacy('old-plan', 'plan'), legacy('agreement', 'spec'), legacy('newer-plan', 'plan', { updatedAt: 5 })]);
  assert.deepEqual(ws.specs.map((spec) => spec.doc.id), ['agreement']);
  assert.deepEqual(ws.specs[0].plans, []);
  assert.deepEqual(ws.needsReview.map((doc) => doc.id), ['newer-plan', 'old-plan']);
  for (const doc of ws.needsReview) {
    assert.equal(doc.type, 'plan');
    assert.equal(doc.parentSpecId, null);
  }
});

test('legacy fields carry over and sessions become a list', () => {
  const ws = workspaceFromLegacy([
    legacy('p', 'plan', { sessions: ' a  b c ', approvedVersion: 3, latestVersion: 12, latestStatus: 'change_requested', latestSource: 'drift', revisionCount: 12 }),
  ]);
  const doc = ws.needsReview[0];
  assert.deepEqual(doc.sessions, ['a', 'b', 'c']);
  assert.equal(doc.title, 'Title p');
  assert.equal(doc.path, 'docs/specs/p/SPEC.md');
  assert.equal(doc.status, 'change_requested');
  assert.equal(doc.latestVersion, 12);
  assert.equal(doc.approvedVersion, 3);
  assert.equal(doc.source, 'drift');
  assert.equal(doc.revisionCount, 12);
});

test('specifications are listed newest first', () => {
  const ws = workspaceFromLegacy([
    legacy('older', 'spec', { title: 'A older', updatedAt: 1 }),
    legacy('newest', 'spec', { title: 'Z newest', updatedAt: 9 }),
    legacy('middle', 'spec', { title: 'M middle', updatedAt: 5 }),
  ]);
  assert.deepEqual(ws.specs.map((spec) => spec.doc.id), ['newest', 'middle', 'older']);
});

test('a legacy document without a title shows its id', () => {
  const ws = workspaceFromLegacy([legacy('untitled', 'spec', { title: '' })]);
  assert.equal(ws.specs[0].doc.title, 'untitled');
});

test('stored plans in the list nest under their spec', () => {
  const ws = workspaceFromLegacy([
    legacy('master', 'spec', {
      plans: [
        { id: 'plan-a', title: 'Plan A', latestVersion: 2, latestStatus: 'approved' },
        { id: 'plan-b', title: 'Plan B', latestVersion: 1, latestStatus: 'draft' },
      ],
    }),
  ]);
  assert.deepEqual(
    ws.specs[0].plans.map((plan) => [plan.id, plan.title, plan.latestVersion, plan.status, plan.parentSpecId, plan.relation, plan.type]),
    [
      ['plan-a', 'Plan A', 2, 'approved', 'master', 'unclassified', 'plan'],
      ['plan-b', 'Plan B', 1, 'draft', 'master', 'unclassified', 'plan'],
    ],
  );
  assert.deepEqual(ws.all.map((doc) => doc.id), ['master', 'plan-a', 'plan-b']);
  assert.deepEqual(ws.needsReview, []);
});

test('hierarchy specs keep their plans nested with one identity', () => {
  const ws = workspaceFromHierarchy([
    { specId: 'spec-b', plans: [] },
    { specId: 'spec-a', plans: [{ planId: 'p1', latestVersion: 2 }, { planId: 'p2', latestVersion: 1 }] },
  ]);
  assert.deepEqual(ws.specs.map((spec) => spec.doc.id), ['spec-a', 'spec-b']);
  const a = ws.specs[0];
  assert.equal(a.doc.type, 'spec');
  assert.deepEqual(a.plans.map((plan) => plan.id), ['p1', 'p2']);
  assert.deepEqual(a.plans.map((plan) => plan.parentSpecId), ['spec-a', 'spec-a']);
  assert.deepEqual(a.plans.map((plan) => plan.latestVersion), [2, 1]);
  assert.deepEqual(ws.needsReview, []);
});

test('plans without a stored relation are unclassified', () => {
  const ws = workspaceFromHierarchy([{ specId: 's', plans: [{ planId: 'p1', latestVersion: 1 }, { planId: 'p2', latestVersion: 1 }] }]);
  assert.deepEqual(ws.specs[0].plans.map((plan) => plan.relation), ['unclassified', 'unclassified']);
  const legacyWs = workspaceFromLegacy([legacy('p', 'plan')]);
  assert.equal(legacyWs.needsReview[0].relation, 'unclassified');
  assert.equal(ws.specs[0].doc.relation, null);
});

test('search keeps a spec when only one of its plans matches', () => {
  const ws = workspaceFromHierarchy([
    { specId: 'billing', plans: [{ planId: 'ledger-first', latestVersion: 1 }, { planId: 'invoices-first', latestVersion: 1 }] },
    { specId: 'search', plans: [] },
  ]);
  const found = filterWorkspace(ws, 'LEDGER', 'all');
  assert.deepEqual(found.specs.map((spec) => spec.doc.id), ['billing']);
  assert.deepEqual(found.specs[0].plans.map((plan) => plan.id), ['ledger-first']);
});

test('search on a spec keeps all its plans', () => {
  const ws = workspaceFromHierarchy([{ specId: 'billing', plans: [{ planId: 'a', latestVersion: 1 }, { planId: 'b', latestVersion: 1 }] }]);
  const found = filterWorkspace(ws, 'billing', 'all');
  assert.deepEqual(found.specs[0].plans.map((plan) => plan.id), ['a', 'b']);
});

test('search matches the path and filters Needs review and all documents', () => {
  const ws = workspaceFromLegacy([legacy('alpha', 'plan', { path: 'docs/specs/vault-folders/SPEC.md' }), legacy('beta', 'plan')]);
  const found = filterWorkspace(ws, 'vault-folders', 'all');
  assert.deepEqual(found.needsReview.map((doc) => doc.id), ['alpha']);
  assert.deepEqual(found.all.map((doc) => doc.id), ['alpha']);
});

test('the status filter applies to every section', () => {
  const ws = workspaceFromLegacy([
    legacy('s-approved', 'spec', { latestStatus: 'approved' }),
    legacy('s-draft', 'spec'),
    legacy('p-approved', 'plan', { latestStatus: 'approved' }),
    legacy('p-drift', 'plan', { latestStatus: 'change_requested' }),
  ]);
  const approved = filterWorkspace(ws, '', 'approved');
  assert.deepEqual(approved.specs.map((spec) => spec.doc.id), ['s-approved']);
  assert.deepEqual(approved.needsReview.map((doc) => doc.id), ['p-approved']);
  assert.deepEqual(approved.all.map((doc) => doc.id).sort(), ['p-approved', 's-approved']);
});

test('all documents lists every document once', () => {
  const ws = workspaceFromHierarchy([
    { specId: 's1', plans: [{ planId: 'p1', latestVersion: 1 }] },
    { specId: 's2', plans: [{ planId: 'p2', latestVersion: 1 }, { planId: 'p3', latestVersion: 1 }] },
  ]);
  assert.deepEqual(ws.all.map((doc) => doc.id), ['s1', 'p1', 's2', 'p2', 'p3']);
  const legacyWs = workspaceFromLegacy([legacy('a', 'spec'), legacy('b', 'plan')]);
  assert.deepEqual(legacyWs.all.map((doc) => doc.id), ['a', 'b']);
});

test('an empty payload gives an empty workspace', () => {
  for (const ws of [workspaceFromLegacy([]), workspaceFromHierarchy([])]) {
    assert.deepEqual(ws, { specs: [], needsReview: [], all: [] });
  }
});

test('opening a spec unfolds it, and opening a plan unfolds its spec', () => {
  const ws = workspaceFromLegacy([
    legacy('master', 'spec', { plans: [{ id: 'p1', title: 'P1', latestVersion: 1, latestStatus: 'draft' }] }),
  ]);
  assert.deepEqual([...expandOnOpen(new Set(), ws.specs[0].doc)], ['master']);
  assert.deepEqual([...expandOnOpen(new Set(['other']), ws.specs[0].plans[0])].sort(), ['master', 'other']);
  const reviewPlan = workspaceFromLegacy([legacy('loose', 'plan')]).needsReview[0];
  assert.deepEqual([...expandOnOpen(new Set(), reviewPlan)], []);
});

test('checklist lines are counted, not tracked', () => {
  assert.equal(checklistLineCount('# T\n- [ ] one\n- [x] two\n  * [X] nested\ntext - [ ] inline\n- [] not a box\n'), 3);
  assert.equal(checklistLineCount(''), 0);
});
