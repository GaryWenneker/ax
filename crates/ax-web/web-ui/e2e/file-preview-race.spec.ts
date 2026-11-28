import { expect, test, type Page } from '@playwright/test';

const file = (path: string) => ({ path, language: 'typescript', size: 10, node_count: 1, indexed_at: 1_790_000_000_000 });
const node = (name: string, path: string) => ({
  id: `${path}::${name}`, kind: 'function', name, qualified_name: name, file_path: path,
  language: 'typescript', start_line: 1, end_line: 2, signature: null, is_exported: 1,
});
const slow = () => new Promise((resolve) => setTimeout(resolve, 1_500));

// Every answer for slow-alpha.ts arrives 1.5 s late; fast-beta.ts answers at once.
async function serveFiles(page: Page): Promise<void> {
  await page.route('**/api/**', async (route) => {
    const url = new URL(route.request().url());
    if (url.pathname === '/api/files/roots') return route.fulfill({ json: { roots: [], files: [] } });
    if (url.pathname === '/api/files') {
      return route.fulfill({ json: { files: [file('fast-beta.ts'), file('slow-alpha.ts')], total: 2 } });
    }
    if (url.pathname === '/api/nodes') {
      const path = url.searchParams.get('file') ?? '';
      if (path === 'slow-alpha.ts') await slow();
      const name = path === 'slow-alpha.ts' ? 'alphaSymbol' : 'betaSymbol';
      return route.fulfill({ json: { nodes: [node(name, path)], total: 1 } });
    }
    if (url.pathname === '/api/source') {
      const path = url.searchParams.get('path') ?? '';
      if (path === 'slow-alpha.ts') await slow();
      const text = path === 'slow-alpha.ts' ? 'const alphaSource = 1;' : 'const betaSource = 2;';
      return route.fulfill({ json: { path, from: 1, to: 1, total_lines: 1, lines: [{ no: 1, text }] } });
    }
    if (url.pathname === '/api/stats') {
      return route.fulfill({ json: { project_name: 'Race', node_count: 2, edge_count: 0, file_count: 2, languages: [], db_size_bytes: 0 } });
    }
    return route.fulfill({ json: {} });
  });
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto('/files');
  await page.getByRole('searchbox', { name: 'Filter by path…' }).fill('.ts');
}

test('a slow symbol response for an earlier file is ignored', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  await serveFiles(page);
  await page.getByText('slow-alpha.ts', { exact: true }).first().click();
  await page.getByText('fast-beta.ts', { exact: true }).first().click();
  const preview = page.locator('.files-preview-pane');
  await expect(preview.getByText('betaSymbol').first()).toBeVisible();
  await page.waitForTimeout(2_000);
  await expect(preview.locator('.files-preview-path')).toHaveText('fast-beta.ts');
  await expect(preview.getByText('alphaSymbol')).toHaveCount(0);
  await expect(preview.getByText('betaSymbol').first()).toBeVisible();
});

test('a slow source response for an earlier file is ignored', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  await serveFiles(page);
  await page.getByText('fast-beta.ts', { exact: true }).first().click();
  const preview = page.locator('.files-preview-pane');
  await preview.getByRole('tab', { name: 'Source' }).click();
  await expect(preview.getByText('betaSource')).toBeVisible();
  await page.getByText('slow-alpha.ts', { exact: true }).first().click();
  await page.getByText('fast-beta.ts', { exact: true }).first().click();
  await expect(preview.getByText('betaSource')).toBeVisible();
  await page.waitForTimeout(2_000);
  await expect(preview.locator('.files-preview-path')).toHaveText('fast-beta.ts');
  await expect(preview.getByText('alphaSource')).toHaveCount(0);
});
