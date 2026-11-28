import { expect, test, type Page } from '@playwright/test';

import { graphStreamBody, mockApi, settledGraphNodes } from './fixtures';

const names = ['alpha', 'beta', 'gamma', 'delta'];
const nodes = names.map((name, i) => ({
  id: `n${i}`,
  name,
  kind: 'function',
  file_path: `src/${name}.rs`,
  community_id: 0,
  degree: i === 0 ? names.length - 1 : 1,
}));
const edges = names.slice(1).map((_, i) => ({ source: 'n0', target: `n${i + 1}`, kind: 'calls' }));
const detail = (n: (typeof nodes)[number]) => ({
  node: {
    id: n.id,
    kind: n.kind,
    name: n.name,
    qualified_name: n.name,
    file_path: n.file_path,
    language: 'rust',
    start_line: 1,
    end_line: 2,
    signature: `fn ${n.name}()`,
    docstring: null,
    visibility: 'pub',
    is_exported: 1,
    is_async: 0,
  },
  callers: [],
  callees: [],
});

/** Count full canvas clears (one per graph redraw). */
async function countRedraws(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const w = window as Window & { axClears?: number };
    w.axClears = 0;
    const clear = CanvasRenderingContext2D.prototype.clearRect;
    CanvasRenderingContext2D.prototype.clearRect = function (...args: Parameters<typeof clear>) {
      w.axClears = (w.axClears ?? 0) + 1;
      return clear.apply(this, args);
    };
  });
}

async function redrawsDuring(page: Page, ms: number): Promise<number> {
  const before = await page.evaluate(() => (window as Window & { axClears?: number }).axClears ?? 0);
  await page.waitForTimeout(ms);
  const after = await page.evaluate(() => (window as Window & { axClears?: number }).axClears ?? 0);
  return after - before;
}

async function selectNode(page: Page): Promise<void> {
  await mockApi(page, {
    ...Object.fromEntries(nodes.map((n) => [`/api/node/${n.id}`, detail(n)])),
    '/api/source': { path: 'src/x.rs', from: 1, to: 2, total_lines: 2, lines: [] },
    '/api/graph/stream': (route) => route.fulfill({ contentType: 'text/event-stream', body: graphStreamBody(nodes, edges) }),
    '/api/graph': { nodes, edges, total_nodes: nodes.length, truncated: false },
    '/api/insights': { communities: [], god_nodes: [], surprising: [] },
  });
  await page.goto('/graph');
  const found = await settledGraphNodes(page);
  expect(found.length).toBeGreaterThan(0);
  const node = found[0];
  await page.mouse.move(node.x, node.y);
  await expect(page.locator('.graph-canvas-wrap')).toHaveAttribute('data-hover', node.key);
  await page.mouse.down();
  await page.mouse.up();
  await expect(page.getByRole('complementary', { name: node.key })).toBeVisible();
}

test.describe('graph selection ring', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop pointer');

  test('stops redrawing once the selection pulse has finished', async ({ page }) => {
    await countRedraws(page);
    await selectNode(page);
    // Three pulses of 1.6 s, plus slack for the blade and layout to settle.
    await page.waitForTimeout(6000);
    expect(await redrawsDuring(page, 1000)).toBeLessThan(5);
  });

  test('does not animate the ring under reduced motion', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await countRedraws(page);
    await selectNode(page);
    await page.waitForTimeout(1500);
    expect(await redrawsDuring(page, 1000)).toBeLessThan(5);
  });
});
