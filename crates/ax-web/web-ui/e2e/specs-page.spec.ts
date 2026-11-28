import { expect, test, type Page } from '@playwright/test';

import { mockApi } from './fixtures';

const spec = (id: string, title: string, kind: string, latestStatus: string) => ({
  id,
  projectId: 'p',
  title,
  path: `docs/specs/${id}/SPEC.md`,
  sessions: '',
  approvedVersion: latestStatus === 'approved' ? 1 : null,
  latestStatus,
  revisionCount: 2,
  kind,
  updatedAt: 1_700_000_000,
  latestVersion: 2,
});

const revisions = (id: string) => [
  { version: 1, status: 'approved', body: `First body of ${id}.`, createdAt: 1_700_000_000, source: 'file', hash: 'a', basedOn: null },
  { version: 2, status: 'draft', body: `Second body of ${id}.`, createdAt: 1_700_000_100, source: 'web', hash: 'b', basedOn: 1 },
];

async function setup(page: Page, path: string): Promise<void> {
  await mockApi(page, {
    '/api/specs': {
      specs: [
        spec('alpha', 'Alpha spec', 'spec', 'approved'),
        spec('beta', 'Beta plan', 'plan', 'draft'),
        spec('gamma', 'Gamma checkout', 'spec', 'change_requested'),
      ],
    },
    '/api/specs/alpha/revisions': { revisions: revisions('alpha') },
    '/api/specs/beta/revisions': { revisions: revisions('beta') },
    '/api/specs/gamma/revisions': { revisions: revisions('gamma') },
  });
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto(path);
}

const blade = (page: Page) => page.locator('.specs-blade');
const source = (page: Page) => blade(page).locator('textarea').first();
const row = (page: Page, title: string) => page.getByText(title, { exact: true }).first();

test.describe('Specs documents view', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop blade');

  test('kind, status and search filters narrow the list', async ({ page }) => {
    await setup(page, '/specs?view=documents');
    await expect(row(page, 'Alpha spec')).toBeVisible();
    await page.getByRole('group', { name: 'Kind' }).getByRole('button', { name: 'Plans', exact: true }).click();
    await expect(row(page, 'Beta plan')).toBeVisible();
    await expect(page.getByText('Alpha spec', { exact: true })).toHaveCount(0);
    await page.getByRole('group', { name: 'Kind' }).getByRole('button', { name: 'All', exact: true }).click();
    await page.getByRole('textbox', { name: 'Search specs' }).fill('  CHECKOUT ');
    await expect(row(page, 'Gamma checkout')).toBeVisible();
    await expect(page.getByText('Alpha spec', { exact: true })).toHaveCount(0);
  });

  test('opening a spec shows the approved version and records it in the address', async ({ page }) => {
    await setup(page, '/specs?view=documents');
    await row(page, 'Alpha spec').click();
    await blade(page).getByRole('button', { name: 'Markdown', exact: true }).click();
    await expect(source(page)).toHaveValue('First body of alpha.');
    await expect(page).toHaveURL(/[?&]spec=alpha(&|$)/);
    await expect(page).toHaveURL(/view=documents/);
    await expect(page).not.toHaveURL(/tab=/);
  });

  test('a spec named in the address opens on load', async ({ page }) => {
    await setup(page, '/specs?view=documents&spec=gamma');
    await expect(blade(page)).toBeVisible();
    await blade(page).getByRole('button', { name: 'Markdown', exact: true }).click();
    await expect(source(page)).toHaveValue('First body of gamma.');
  });
});

test.describe('Specs workspace view', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop panes');

  test('opens by default with an explorer, a document pane and a context panel', async ({ page }) => {
    await setup(page, '/specs');
    await expect(page.getByRole('heading', { name: 'Specs & Plans' })).toBeVisible();
    await expect(page.getByRole('region', { name: 'Specification explorer' })).toBeVisible();
    await expect(page.getByRole('region', { name: 'Document', exact: true })).toBeVisible();
    await expect(page.getByRole('region', { name: 'Context and evidence' })).toBeVisible();
  });

  test('opening a spec from the explorer shows its tabs and records it in the address', async ({ page }) => {
    await setup(page, '/specs');
    await page.getByRole('region', { name: 'Specification explorer' }).getByText('Alpha spec', { exact: true }).first().click();
    await expect(page).toHaveURL(/[?&]spec=alpha(&|$)/);
    for (const name of ['Overview', 'Content', 'History']) {
      await expect(page.getByRole('tab', { name, exact: true })).toBeVisible();
    }
  });
});
