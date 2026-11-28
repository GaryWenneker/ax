import { expect, test } from '@playwright/test';

const recent = [
  { path: '/work/alpha', label: 'alpha', last_opened: 2, initialized: true },
  { path: '/work/beta', label: 'beta', last_opened: 1, initialized: true },
];

// Trash on a recent project closes the Switch project list and opens Remove ax data for that project.
test('trash on a recent project closes the switch list and opens the remove dialog for it', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  await page.route('**/api/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/workspace/current') {
      return route.fulfill({ json: { ok: true, workspace: { path: '/work/alpha', label: 'alpha', initialized: true }, recent } });
    }
    if (path === '/api/workspace/recent') return route.fulfill({ json: { ok: true, recent } });
    if (path === '/api/workspace/purge-plan') return route.fulfill({ json: { ok: true, path: '/work/beta', exists: true, groups: [] } });
    if (path === '/api/files/roots') return route.fulfill({ json: { roots: [], files: [] } });
    if (path === '/api/files') return route.fulfill({ json: { files: [], total: 0 } });
    if (path === '/api/stats') {
      return route.fulfill({ json: { project_name: 'alpha', node_count: 0, edge_count: 0, file_count: 0, languages: [], db_size_bytes: 0 } });
    }
    return route.fulfill({ json: {} });
  });
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto('/files');

  const trigger = page.locator('.titlebar-tools .logging-proj-switch-btn');
  await expect(trigger).toHaveText(/alpha/);
  await trigger.click();
  const menu = page.locator('.logging-proj-switch-menu');
  await expect(menu).toBeVisible();

  await menu.getByRole('button', { name: 'Remove ax data for beta' }).click();

  await expect(menu).toHaveCount(0);
  await expect(trigger).toHaveAttribute('aria-expanded', 'false');
  await expect(page.getByRole('dialog', { name: 'Remove ax data for beta' })).toBeVisible();
});
