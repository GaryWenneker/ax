import { expect, test, type Page } from '@playwright/test';

const file = (path: string) => ({ path, language: 'typescript', size: 10, node_count: 1, indexed_at: 1_790_000_000_000 });

// A filter match deep in the tree; browse mode shows only the top-level root.
async function serveFiles(page: Page, prefixRequests: string[]): Promise<void> {
  await page.route('**/api/**', async (route) => {
    const url = new URL(route.request().url());
    if (url.pathname === '/api/files/roots') {
      return route.fulfill({ json: { roots: [{ name: 'pkg', path: 'pkg', count: 1 }], files: [] } });
    }
    if (url.pathname === '/api/files') {
      const prefix = url.searchParams.get('prefix');
      if (prefix !== null) {
        prefixRequests.push(prefix);
        return route.fulfill({ json: { files: [], total: 0 } });
      }
      if (url.searchParams.get('q')) {
        return route.fulfill({ json: { files: [file('pkg/deep/nested/match.ts')], total: 1 } });
      }
      return route.fulfill({ json: { files: [], total: 0 } });
    }
    if (url.pathname === '/api/stats') {
      return route.fulfill({ json: { project_name: 'Tree', node_count: 1, edge_count: 0, file_count: 1, languages: [], db_size_bytes: 0 } });
    }
    return route.fulfill({ json: {} });
  });
}

test('filter-mode expansion is not saved as the browse-mode tree state', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  const prefixRequests: string[] = [];
  await serveFiles(page, prefixRequests);
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto('/files');
  await page.evaluate(() => localStorage.clear());
  await page.reload();

  const filter = page.getByRole('searchbox', { name: 'Filter by path…' });
  await filter.fill('match');
  await expect(page.getByText('match.ts', { exact: true }).first()).toBeVisible();

  await filter.fill('');
  await expect(page.getByText('pkg', { exact: true }).first()).toBeVisible();
  await page.waitForTimeout(800);

  const stored = await page.evaluate(() => {
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i) ?? '';
      if (key.endsWith('files-tree-expanded-v2')) return JSON.parse(localStorage.getItem(key) ?? '[]') as string[];
    }
    return [];
  });
  expect(stored).toEqual([]);
  // Clearing the filter must not lazy-load every folder the filter opened.
  expect(prefixRequests).toEqual([]);
  await expect(page.getByText('match.ts', { exact: true })).toHaveCount(0);

  await page.reload();
  await expect(page.getByText('pkg', { exact: true }).first()).toBeVisible();
  await page.waitForTimeout(500);
  expect(prefixRequests).toEqual([]);
});

test('a folder opened in browse mode stays open across a filter and a reload', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  const prefixRequests: string[] = [];
  await serveFiles(page, prefixRequests);
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto('/files');
  await page.evaluate(() => localStorage.clear());
  await page.reload();

  await page.getByText('pkg', { exact: true }).first().click();
  await expect.poll(() => prefixRequests).toContain('pkg');

  const filter = page.getByRole('searchbox', { name: 'Filter by path…' });
  await filter.fill('match');
  await expect(page.getByText('match.ts', { exact: true }).first()).toBeVisible();
  await filter.fill('');
  await page.waitForTimeout(500);

  const stored = await page.evaluate(() => {
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i) ?? '';
      if (key.endsWith('files-tree-expanded-v2')) return JSON.parse(localStorage.getItem(key) ?? '[]') as string[];
    }
    return [];
  });
  expect(stored).toEqual(['pkg']);
});
