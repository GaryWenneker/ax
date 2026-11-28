import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { pinNode, releaseNode } from './graphDrag.ts';

test('pinning a node that no longer exists is a no-op instead of a TypeError', () => {
  const nodes: { fx?: number | null; fy?: number | null }[] = [{}];
  assert.equal(pinNode(nodes, 3, 10, 20), false);
  assert.equal(pinNode(nodes, 0, 10, 20), true);
  assert.deepEqual(nodes[0], { fx: 10, fy: 20 });
});

test('releasing a node clears its pin and tolerates a missing node', () => {
  const nodes: { fx?: number | null; fy?: number | null }[] = [{ fx: 1, fy: 2 }];
  assert.equal(releaseNode(nodes, 5), false);
  assert.equal(releaseNode(nodes, 0), true);
  assert.deepEqual(nodes[0], { fx: null, fy: null });
});

test('the canvas ends a drag on pointer cancel, lost capture and a layout restart', () => {
  const source = readFileSync(new URL('../components/ForceGraphCanvas.tsx', import.meta.url), 'utf8');
  assert.match(source, /onPointerCancel=\{endDrag\}/);
  assert.match(source, /onLostPointerCapture=\{endDrag\}/);
  const restart = source.slice(source.indexOf('const restart = useCallback'), source.indexOf('useEffect(() => {\n    restart(0.9)'));
  assert.match(restart, /drag\.current = null/);
  assert.doesNotMatch(source, /const n = simNodesRef\.current\[d\.node\];\s*\n\s*n\.f/);
});
