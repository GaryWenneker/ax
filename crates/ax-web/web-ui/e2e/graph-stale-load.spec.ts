import { expect, test } from '@playwright/test';

import { graphStreamBody, mockApi } from './fixtures';

const nodes = ['alpha', 'beta', 'gamma'].map((name, i) => ({
  id: `code-${i}`,
  name,
  kind: 'function',
  file_path: `src/${name}.rs`,
  community_id: 0,
  degree: 2,
}));
const edges = [
  { source: 'code-0', target: 'code-1', kind: 'calls' },
  { source: 'code-0', target: 'code-2', kind: 'calls' },
];
const overlay = {
  version: 1,
  nodes: [
    { id: 'domain-a', kind: 'domain', name: 'Billing' },
    { id: 'domain-b', kind: 'domain', name: 'Search' },
  ],
  edges: [{ source: 'domain-a', target: 'domain-b', kind: 'cross_domain' }],
};

test('a domain overlay that answers after the user went back to Structure is dropped', async ({ page }) => {
  await mockApi(page, {
    '/api/graph/stream': (route) => route.fulfill({ contentType: 'text/event-stream', body: graphStreamBody(nodes, edges) }),
    '/api/graph': { nodes, edges, total_nodes: nodes.length, truncated: false },
    '/api/insights': { communities: [], god_nodes: [], surprising: [] },
    '/api/domain-graph': async (route) => {
      await new Promise((resolve) => setTimeout(resolve, 1500));
      await route.fulfill({ json: overlay });
    },
  });
  await page.goto('/graph');
  const wrap = page.locator('.graph-canvas-wrap');
  await expect(wrap).toHaveAttribute('data-settled', 'true', { timeout: 20_000 });

  await page.getByRole('tab', { name: 'Domain' }).click();
  await page.getByRole('tab', { name: 'Structure' }).click();
  // Let the slow overlay answer land.
  await page.waitForTimeout(2500);

  const keys = await wrap.evaluate((el) => {
    const probe = el as HTMLElement & { axGraphNodes?: () => { key: string }[] };
    return (probe.axGraphNodes?.() ?? []).map((n) => n.key).sort();
  });
  expect(keys).not.toContain('Billing');
  expect(keys).toEqual(nodes.map((n) => n.name).sort());
});
