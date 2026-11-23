import assert from 'node:assert/strict';
import test from 'node:test';
import {
  hierarchyRows,
  hierarchySpecRows,
  parseSpecPayload,
  plansForSpec,
} from './hierarchyList.ts';

test('a flat spec payload stays a flat list', () => {
  const parsed = parseSpecPayload({ specs: [{ id: 'spec-billing', title: 'Billing' }] });
  assert.equal(parsed.mode, 'legacy');
  if (parsed.mode !== 'legacy') return;
  assert.equal(parsed.specs.length, 1);
});

test('a hierarchy payload lists the spec and then its plan without the body', () => {
  const parsed = parseSpecPayload({
    mode: 'hierarchy',
    projectId: 'ax',
    specs: [
      {
        specId: 'spec-billing',
        plans: [{ planId: 'plan-ledger', latestVersion: 2, body: 'Record the ledger.' }],
      },
    ],
  });
  assert.equal(parsed.mode, 'hierarchy');
  if (parsed.mode !== 'hierarchy') return;
  assert.equal(parsed.projectId, 'ax');
  assert.deepEqual(parsed.nodes, [
    { specId: 'spec-billing', plans: [{ planId: 'plan-ledger', latestVersion: 2 }] },
  ]);
  const rows = hierarchyRows(parsed.nodes);
  assert.deepEqual(rows, [
    { key: 'spec-billing', specId: 'spec-billing', title: 'spec-billing', kind: 'spec', version: null },
    {
      key: 'spec-billing/plan-ledger',
      specId: 'spec-billing',
      title: 'plan-ledger',
      kind: 'plan',
      version: 2,
    },
  ]);
  assert.equal(JSON.stringify(rows).includes('Record the ledger.'), false);
});

test('hierarchySpecRows shows specs only so plans stay under the opened spec', () => {
  const nodes = [
    {
      specId: 'spec-billing',
      plans: [
        { planId: 'plan-ledger', latestVersion: 2 },
        { planId: 'plan-alt', latestVersion: 1 },
      ],
    },
  ];
  assert.deepEqual(hierarchySpecRows(nodes), [
    { key: 'spec-billing', specId: 'spec-billing', title: 'spec-billing', kind: 'spec', version: null },
  ]);
  assert.deepEqual(plansForSpec(nodes, 'spec-billing'), [
    { planId: 'plan-ledger', latestVersion: 2 },
    { planId: 'plan-alt', latestVersion: 1 },
  ]);
  assert.deepEqual(plansForSpec(nodes, 'missing'), []);
});
