import { expect, test, type Page } from '@playwright/test';

import { mockApi } from './fixtures';

const spec = (id: string, title: string) => ({
  id,
  projectId: 'p',
  title,
  path: `docs/specs/${id}/SPEC.md`,
  sessions: '',
  approvedVersion: 1,
  latestStatus: 'approved',
  revisionCount: 1,
  kind: 'spec',
  updatedAt: 1_700_000_000,
  latestVersion: 1,
});

const revision = (id: string) => ({
  version: 1,
  status: 'approved',
  body: `Saved body of ${id}.`,
  createdAt: 1_700_000_000,
  source: 'file',
  hash: 'h',
  basedOn: null,
});

async function openDocuments(page: Page): Promise<void> {
  await mockApi(page, {
    '/api/specs': { specs: [spec('alpha', 'Alpha spec'), spec('beta', 'Beta spec')] },
    '/api/specs/alpha/revisions': { revisions: [revision('alpha')] },
    '/api/specs/beta/revisions': { revisions: [revision('beta')] },
  });
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto('/specs?view=documents');
  await page.getByText('Alpha spec', { exact: true }).first().click();
}

const blade = (page: Page) => page.locator('.specs-blade');
const source = (page: Page) => blade(page).locator('textarea').first();

test.describe('Specs documents view keeps unsaved edits', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop blade');

  async function edit(page: Page): Promise<void> {
    await openDocuments(page);
    await blade(page).getByRole('button', { name: 'Markdown', exact: true }).click();
    await expect(source(page)).toHaveValue('Saved body of alpha.');
    await source(page).fill('A long draft the user typed.');
    await expect(page.getByRole('status').filter({ hasText: 'Unsaved changes' })).toBeVisible();
  }

  async function expectDraftKept(page: Page): Promise<void> {
    await expect(page.getByRole('status').filter({ hasText: 'Discard or save unsaved changes' })).toBeVisible();
    await page.waitForTimeout(400);
    await expect(blade(page)).toBeVisible();
    await expect(source(page)).toHaveValue('A long draft the user typed.');
  }

  test('a click outside the blade does not close it', async ({ page }) => {
    await edit(page);
    await page.getByRole('group', { name: 'Kind' }).getByRole('button', { name: 'All', exact: true }).click();
    await expectDraftKept(page);
  });

  test('Escape does not close the blade', async ({ page }) => {
    await edit(page);
    await page.getByRole('textbox', { name: 'Search specs' }).press('Escape');
    await expectDraftKept(page);
  });

  test('the close button does not drop the draft', async ({ page }) => {
    await edit(page);
    await blade(page).getByRole('button', { name: 'Close' }).click();
    await expectDraftKept(page);
  });

  test('opening another row does not drop the draft', async ({ page }) => {
    await edit(page);
    await page.getByText('Beta spec', { exact: true }).first().click();
    await expectDraftKept(page);
    await expect(page).toHaveURL(/spec=alpha/);
  });
});
