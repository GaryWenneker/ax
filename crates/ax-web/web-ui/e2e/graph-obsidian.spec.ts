import { expect, test, type Page } from '@playwright/test';

import { graphStreamBody, mockApi, settledGraphNodes } from './fixtures';

const names = ['parse_config', 'load_index', 'run_query', 'render_page', 'save_cache', 'open_store', 'close_store', 'emit_event'];
const nodes = names.map((name, i) => ({
  id: `n${i}`,
  name,
  kind: 'function',
  file_path: `src/${name}.rs`,
  community_id: i % 2,
  degree: i === 0 ? names.length - 1 : 1,
}));
const edges = names.slice(1).map((_, i) => ({ source: 'n0', target: `n${i + 1}`, kind: 'calls' }));

const nodeDetail = (n: (typeof nodes)[number]) => ({
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

async function serveGraph(page: Page): Promise<void> {
  await mockApi(page, {
    ...Object.fromEntries(nodes.map((n) => [`/api/node/${n.id}`, nodeDetail(n)])),
    '/api/source': { path: 'src/x.rs', from: 1, to: 2, total_lines: 2, lines: [] },
    '/api/graph/stream': (route) =>
      route.fulfill({ contentType: 'text/event-stream', body: graphStreamBody(nodes, edges) }),
    '/api/graph': { nodes, edges, total_nodes: nodes.length, truncated: false },
    '/api/insights': { communities: [], god_nodes: [], surprising: [] },
  });
}

/** A node in the middle of the canvas, away from the side panels. */
async function pickNode(page: Page) {
  const found = await settledGraphNodes(page);
  expect(found.length).toBeGreaterThan(0);
  return found[0];
}

test.describe('Obsidian-style graph settings', () => {
  test.beforeEach(async ({ page }) => {
    await serveGraph(page);
    await page.goto('/graph');
    await page.evaluate(() => localStorage.removeItem('ax.graph.settings'));
    await page.reload();
  });

  const openSettings = async (page: Page) => {
    await page.getByRole('button', { name: 'Graph settings' }).click();
    return page.getByRole('dialog', { name: 'Graph settings' });
  };

  test('settings start closed, open via the button, close on outside click', async ({ page }) => {
    const panel = page.getByRole('dialog', { name: 'Graph settings' });
    await expect(panel).toHaveCount(0);
    await openSettings(page);
    await expect(panel).toBeVisible();
    await panel.getByLabel('Node size').click();
    await expect(panel).toBeVisible();
    const box = (await page.locator('canvas.graph-canvas').boundingBox())!;
    await page.mouse.click(box.x + 20, box.y + box.height - 20);
    await expect(panel).toHaveCount(0);
  });

  test('panel shows Filters, Groups, Display, and Forces', async ({ page }) => {
    const panel = await openSettings(page);
    await expect(panel).toBeVisible();
    for (const title of ['Filters', 'Groups', 'Display', 'Forces']) {
      await expect(panel.getByRole('button', { name: title, exact: false }).first()).toBeVisible();
    }
    await expect(panel.getByLabel('Repel force')).toHaveValue('10');
    await expect(panel.getByLabel('Link distance')).toHaveValue('250');
  });

  test('hovering a node marks it as hovered', async ({ page }, info) => {
    test.skip(info.project.name === 'mobile-chrome', 'no mouse hover on touch');
    const node = await pickNode(page);
    await page.mouse.move(node.x, node.y);
    await expect(page.locator('.graph-canvas-wrap')).toHaveAttribute('data-hover', node.key);
  });

  test('clicking a node opens a detail blade', async ({ page }, info) => {
    test.skip(info.project.name === 'mobile-chrome', 'desktop pointer');
    const node = await pickNode(page);
    await page.mouse.move(node.x, node.y);
    await expect(page.locator('.graph-canvas-wrap')).toHaveAttribute('data-hover', node.key);
    await page.mouse.down();
    await page.mouse.up();
    const blade = page.getByRole('complementary', { name: node.key });
    await expect(blade).toBeVisible();
    await expect(blade).toContainText(node.key);
    const canvasWidth = (await page.locator('canvas.graph-canvas').boundingBox())!.width;
    expect(canvasWidth).toBeGreaterThan(400);
    await page.getByRole('button', { name: /Show selection/ }).click();
    await expect(blade).toBeVisible();
  });

  test('slider changes persist and reset restores defaults', async ({ page }) => {
    const panel = await openSettings(page);
    await panel.getByLabel('Link distance').fill('400');
    await page.reload();
    await openSettings(page);
    await expect(page.getByRole('dialog', { name: 'Graph settings' }).getByLabel('Link distance')).toHaveValue('400');
    await page.getByTitle('Reset to defaults').click();
    await expect(page.getByRole('dialog', { name: 'Graph settings' }).getByLabel('Link distance')).toHaveValue('250');
  });
});
