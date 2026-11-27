import { expect, test, type Page } from '@playwright/test';

const recent = [
  { path: '/work/alpha', label: 'alpha', last_opened: 2, initialized: true },
  { path: '/work/beta', label: 'beta', last_opened: 1, initialized: true },
];

type Replies = {
  init?: 'no-done' | 'abort';
  plan?: 'abort' | 'html500';
  purge?: 'abort' | 'html500';
};

async function serve(page: Page, replies: Replies): Promise<void> {
  await page.route('**/api/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname;
    if (path === '/api/workspace/current') {
      return route.fulfill({ json: { ok: true, workspace: { path: '/work/alpha', label: 'alpha', initialized: true }, recent } });
    }
    if (path === '/api/workspace/recent') return route.fulfill({ json: { ok: true, recent } });
    if (path === '/api/workspace/browse') {
      return route.fulfill({
        json: { ok: true, path: '/work/fresh', parent: '/work', initialized: false, entries: [] },
      });
    }
    if (path === '/api/workspace/init/stream') {
      if (replies.init === 'abort') return route.abort('connectionreset');
      return route.fulfill({ contentType: 'text/event-stream', body: 'data: {"type":"line","text":"indexing files"}\n\n' });
    }
    if (path === '/api/workspace/purge-plan') {
      if (replies.plan === 'abort') return route.abort('connectionrefused');
      if (replies.plan === 'html500') return route.fulfill({ status: 500, contentType: 'text/html', body: '<h1>oops</h1>' });
      return route.fulfill({
        json: {
          ok: true,
          path: '/work/beta',
          groups: [{ id: 'database', label: 'Database', detail: '', bytes: 10, fileCount: 1, defaultOn: true, empty: false }],
        },
      });
    }
    if (path === '/api/workspace/purge') {
      if (replies.purge === 'abort') return route.abort('connectionreset');
      return route.fulfill({ status: 500, contentType: 'text/html', body: '<h1>Internal Server Error</h1>' });
    }
    if (path === '/api/files/roots') return route.fulfill({ json: { roots: [], files: [] } });
    if (path === '/api/files') return route.fulfill({ json: { files: [], total: 0 } });
    if (path === '/api/stats') {
      return route.fulfill({ json: { project_name: 'alpha', node_count: 0, edge_count: 0, file_count: 0, languages: [], db_size_bytes: 0 } });
    }
    return route.fulfill({ json: {} });
  });
  await page.setViewportSize({ width: 1600, height: 900 });
}

for (const init of ['no-done', 'abort'] as const) {
  test(`ax init that ends without a result (${init}) re-enables the project browser`, async ({ page, isMobile }) => {
    test.skip(isMobile, 'desktop status bar');
    await serve(page, { init });
    await page.goto('/files');
    await page.getByRole('button', { name: /^Switch project/ }).first().click();
    const modal = page.getByRole('dialog', { name: 'Browse projects' });
    const initButton = modal.getByRole('button', { name: 'Initialize (ax init)' });
    await initButton.click();
    await expect(modal.locator('.project-browser-toast--err')).toBeVisible();
    await expect(initButton).toBeEnabled();
    await expect(modal.getByRole('button', { name: 'Open folder' })).toBeEnabled();
  });
}

for (const purge of ['abort', 'html500'] as const) {
  test(`a failed remove (${purge}) shows an error and re-enables the purge dialog`, async ({ page, isMobile }) => {
    test.skip(isMobile, 'desktop layout');
    await serve(page, { purge });
    await page.goto('/files');
    await page.locator('.titlebar-tools .logging-proj-switch-btn').click();
    await page.getByRole('button', { name: 'Remove ax data for beta' }).click();
    const dialog = page.getByRole('dialog', { name: 'Remove ax data for beta' });
    const remove = dialog.getByRole('button', { name: 'Remove selected' });
    await expect(remove).toBeEnabled();
    await remove.click();
    await expect(dialog.getByText(/server/i).first()).toBeVisible();
    await expect(remove).toBeEnabled();
    await expect(dialog.getByRole('button', { name: 'Cancel' })).toBeEnabled();
  });
}

for (const plan of ['abort', 'html500'] as const) {
  test(`a failed purge plan load (${plan}) shows an error instead of loading forever`, async ({ page, isMobile }) => {
    test.skip(isMobile, 'desktop layout');
    await serve(page, { plan });
    await page.goto('/files');
    await page.locator('.titlebar-tools .logging-proj-switch-btn').click();
    await page.getByRole('button', { name: 'Remove ax data for beta' }).click();
    const dialog = page.getByRole('dialog', { name: 'Remove ax data for beta' });
    await expect(dialog.getByText(/server/i).first()).toBeVisible();
  });
}
