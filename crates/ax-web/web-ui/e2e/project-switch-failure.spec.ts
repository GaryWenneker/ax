import { expect, test, type Page } from '@playwright/test';

const recent = [
  { path: '/work/alpha', label: 'alpha', last_opened: 2, initialized: true },
  { path: '/work/beta', label: 'beta', last_opened: 1, initialized: true },
];

async function serve(page: Page, switchReply: 'abort' | 'html500', failRefresh = false): Promise<void> {
  await page.route('**/api/**', async (route) => {
    const url = new URL(route.request().url());
    if (url.pathname === '/api/workspace/current') {
      if (failRefresh) return route.fulfill({ status: 502, contentType: 'text/html', body: '<h1>Bad gateway</h1>' });
      return route.fulfill({ json: { ok: true, workspace: { path: '/work/alpha', label: 'alpha', initialized: true }, recent } });
    }
    if (url.pathname === '/api/workspace/recent') return route.fulfill({ json: { ok: true, recent } });
    if (url.pathname === '/api/workspace/switch') {
      if (switchReply === 'abort') return route.abort('connectionrefused');
      return route.fulfill({ status: 500, contentType: 'text/html', body: '<h1>Internal Server Error</h1>' });
    }
    if (url.pathname === '/api/files/roots') return route.fulfill({ json: { roots: [], files: [] } });
    if (url.pathname === '/api/files') return route.fulfill({ json: { files: [], total: 0 } });
    if (url.pathname === '/api/stats') {
      return route.fulfill({ json: { project_name: 'alpha', node_count: 0, edge_count: 0, file_count: 0, languages: [], db_size_bytes: 0 } });
    }
    return route.fulfill({ json: {} });
  });
  await page.setViewportSize({ width: 1600, height: 900 });
}

for (const reply of ['abort', 'html500'] as const) {
  test(`a failed project switch (${reply}) re-enables the switcher and shows an error`, async ({ page, isMobile }) => {
    test.skip(isMobile, 'desktop layout');
    await serve(page, reply);
    await page.goto('/files');
    const trigger = page.locator('.titlebar-tools .logging-proj-switch-btn');
    await expect(trigger).toHaveText(/alpha/);
    await trigger.click();
    const menu = page.locator('.logging-proj-switch-menu');
    await menu.getByRole('option', { name: /beta/ }).click();
    await expect(menu.locator('.logging-proj-switch-err')).toBeVisible();
    await expect(menu.getByRole('option', { name: /beta/ })).toBeEnabled();
    await expect(trigger).toBeEnabled();
  });
}

test('a failed refresh of the recent list does not leave an unhandled rejection', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await serve(page, 'abort', true);
  await page.goto('/files');
  await expect(page.locator('.titlebar-tools .logging-proj-switch-btn')).toBeEnabled();
  await page.waitForTimeout(500);
  expect(errors.filter((m) => /JSON|Unexpected token/i.test(m))).toEqual([]);
});
