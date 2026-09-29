import { test } from 'node:test';
import assert from 'node:assert/strict';
import { backlinks, outgoing, policyGraphModel, type PolicyGraphPayload } from './policyGraph.ts';

const payload: PolicyGraphPayload = {
  nodes: [
    { key: 'rule:project:a', kind: 'rule', id: 'a', label: 'Rule A', origin: 'project' },
    { key: 'skill:global:b', kind: 'skill', id: 'b', label: 'Skill B', origin: 'global', projectId: 7 },
    { key: 'memory:project:m', kind: 'memory', id: 'm', label: 'Mem', origin: 'project' },
    { key: 'rule:project:lonely', kind: 'rule', id: 'lonely', label: 'Lonely', origin: 'project' },
  ],
  edges: [
    { source: 'rule:project:a', target: 'skill:global:b' },
    { source: 'memory:project:m', target: 'rule:project:a' },
    { source: 'rule:project:a', target: 'rule:project:ghost' },
  ],
};

test('model keeps every node and drops edges to unknown keys', () => {
  const m = policyGraphModel(payload);
  assert.equal(m.nodes.length, 4);
  assert.deepEqual(m.edges, [
    { source: 'rule:project:a', target: 'skill:global:b' },
    { source: 'memory:project:m', target: 'rule:project:a' },
  ]);
});

test('model counts degree and marks global nodes', () => {
  const m = policyGraphModel(payload);
  const byKey = new Map(m.nodes.map((n) => [n.key, n]));
  assert.equal(byKey.get('rule:project:a')!.degree, 2);
  assert.equal(byKey.get('rule:project:lonely')!.degree, 0);
  assert.equal(byKey.get('skill:global:b')!.global, true);
  assert.equal(byKey.get('rule:project:a')!.global, false);
});

test('outgoing and backlinks follow edge direction', () => {
  const m = policyGraphModel(payload);
  assert.deepEqual(outgoing(m, 'rule:project:a').map((n) => n.key), ['skill:global:b']);
  assert.deepEqual(backlinks(m, 'rule:project:a').map((n) => n.key), ['memory:project:m']);
  assert.deepEqual(backlinks(m, 'memory:project:m'), []);
});

test('model tolerates a missing payload', () => {
  const m = policyGraphModel(null);
  assert.deepEqual(m, { nodes: [], edges: [] });
});
