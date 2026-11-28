import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildGrid, dotRadii, gridWeight } from '../src/scripts/grid-backdrop.ts';

test('the grid is calm on the left and strongest toward the right, as on the website', () => {
  assert.ok(gridWeight(0.86, 0.45, 16 / 9) > 0.95);
  assert.ok(gridWeight(0.1, 0.5, 16 / 9) < 0);
  // Portrait: gathered in the top-right corner.
  assert.ok(gridWeight(0.95, 0.08, 0.5) > 0.95);
  assert.ok(gridWeight(0.1, 0.9, 0.5) < 0);
});

test('dots stay inside their cell and the left side stays empty', () => {
  const grid = buildGrid(1600, 900);
  const radii = dotRadii(grid, 3.2);
  const nx = grid.cols.length;
  assert.equal(radii.length, nx * grid.rows.length);
  for (const r of radii) assert.ok(r >= 0 && r <= grid.spacing * 0.38 + 1e-6);
  // The left tenth (behind the copy on the site) has no dots.
  const leftCols = Math.floor(nx / 10);
  for (let j = 0; j < grid.rows.length; j++) {
    for (let i = 0; i < leftCols; i++) assert.equal(radii[j * nx + i], 0);
  }
  assert.ok(radii.some((r) => r > 0));
});

test('the waves move the dots over time and reuse the output buffer', () => {
  const grid = buildGrid(800, 600);
  const a = dotRadii(grid, 0).slice();
  const buffer = new Float32Array(a.length);
  const b = dotRadii(grid, 1.5, buffer);
  assert.equal(b, buffer);
  assert.notDeepEqual(Array.from(a), Array.from(b));
});

test('the homepage and the docs put the grid behind the page, without the ring', async () => {
  const { readFileSync } = await import('node:fs');
  const read = (p: string) => readFileSync(new URL(`../${p}`, import.meta.url), 'utf8');
  const component = read('src/components/GridBackdrop.astro');
  const styles = read('src/styles/grid-backdrop.css');
  const script = read('src/scripts/grid-backdrop.ts');
  assert.match(read('src/components/MinimalHome.astro'), /<GridBackdrop \/>/);
  assert.match(read('src/components/Header.astro'), /<GridBackdrop variant="page" \/>/);
  assert.match(component, /class="ax-grid" data-ax-grid data-ax-variant=\{variant\} aria-hidden="true"/);
  assert.match(component, /mountGridBackdrop\(root\)/);
  assert.match(styles, /\.ax-grid \{[^}]*position: fixed;[^}]*z-index: 0;[^}]*pointer-events: none;/);
  assert.match(styles, /:root\[data-theme="light"\] \.ax-grid \{\s*display: none;/);
  // Still with reduced motion, paused in a hidden tab, and no violet ring.
  assert.match(script, /!reduce\.matches && document\.visibilityState === 'visible'/);
  assert.doesNotMatch(script + styles + component, /silk|#6d28d9|#7c3aed|#a78bfa|#d946ef/i);
});
