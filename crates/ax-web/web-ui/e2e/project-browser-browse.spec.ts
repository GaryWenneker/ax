import { expect, test, type Page } from '@playwright/test';

const recent = [{ path: '/work/alpha', label: 'alpha', last_opened: 2, initialized: true }];

const folders: Record<string, { initialized: boolean; parent: string }> = {
  '/work/alpha': { initialized: true, parent: '/work' },
  '/work/slow': { initialized: true, parent: '/work' },
  '/work/fast': { initialized: false, parent: '/work' },
};

async function serve(page: Page, switched: string[]): Promise<void> {
  await page.route('**/api/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname;
    if (path === '/api/workspace/current') {
      return route.fulfill({ json: { ok: true, workspace: { path: '/work/alpha', label: 'alpha', initialized: true }, recent } });
    }
    if (path === '/api/workspace/recent') return route.fulfill({ json: { ok: true, recent } });
    if (path === '/api/workspace/browse') {
      const target = url.searchParams.get('path') ?? '';
      if (target === '/work/slow') await new Promise((resolve) => setTimeout(resolve, 1500));
      const folder = folders[target];
      if (!folder) return route.fulfill({ json: { ok: false, error: `No such folder: ${target}` } });
      return route.fulfill({
        json: { ok: true, path: target, parent: folder.parent, initialized: folder.initialized, entries: [{ name: 'src', path: `${target}/src`, is_dir: true, initialized: false }] },
      });
    }
    if (path === '/api/workspace/switch') {
      const body = route.request().postDataJSON() as { path?: string };
      switched.push(body.path ?? '');
      return route.fulfill({ json: { ok: false, error: 'stop here' } });
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

async function openBrowser(page: Page) {
  await page.goto('/files');
  await page.getByRole('button', { name: /^Switch project/ }).first().click();
  const modal = page.getByRole('dialog', { name: 'Browse projects' });
  await expect(modal.getByRole('button', { name: 'Switch to this project' })).toBeVisible();
  return modal;
}

test('a mistyped path keeps the footer aimed at the folder that is shown', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  const switched: string[] = [];
  await serve(page, switched);
  const modal = await openBrowser(page);
  await modal.getByRole('button', { name: 'Edit path' }).click();
  const input = modal.getByRole('textbox', { name: 'Folder path' });
  await input.fill('/work/alpah');
  await modal.getByRole('button', { name: 'Go' }).click();
  await expect(modal.locator('.project-browser-toast--err')).toContainText('No such folder: /work/alpah');
  await expect(input).toHaveValue('/work/alpha');
  await modal.getByRole('button', { name: 'Switch to this project' }).click();
  await expect.poll(() => switched).toEqual(['/work/alpha']);
});

test('a slow earlier browse does not pull the view back', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  await serve(page, []);
  const modal = await openBrowser(page);
  const go = async (path: string) => {
    const input = modal.getByRole('textbox', { name: 'Folder path' });
    if (!(await input.isVisible())) await modal.getByRole('button', { name: 'Edit path' }).click();
    await input.fill(path);
    await modal.getByRole('button', { name: 'Go' }).click();
  };
  await go('/work/slow');
  await go('/work/fast');
  await expect(modal.locator('.project-browser-open-actions').getByRole('button', { name: 'Open folder' })).toBeVisible();
  await page.waitForTimeout(2000);
  await expect(modal.locator('.project-browser-open-actions').getByRole('button', { name: 'Open folder' })).toBeVisible();
  await expect(modal.locator('.project-browser-crumb--current')).toHaveText('fast');
});
