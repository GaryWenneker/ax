import { expect, test, type Page } from '@playwright/test';

// One story, in order, against one running ax-admin: first start, connecting,
// several environments, management, and removing everything again.
test.describe.configure({ mode: 'serial' });

const env = (name: string): string => {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is not set; run tools/gauntlet-ax-admin.sh`);
  return value;
};
const ALPHA = env('AX_E2E_ALPHA_URL');
const BETA = env('AX_E2E_BETA_URL');
const CONTEXT_KEY = env('AX_E2E_CONTEXT_KEY');
const ADMIN_KEY = env('AX_E2E_ADMIN_KEY');
const BETA_KEY = env('AX_E2E_BETA_KEY');

const nav = (page: Page) => page.getByRole('navigation', { name: 'Main navigation' });

// Documentation screenshots, written only when AX_E2E_SCREENSHOTS names a folder.
async function shot(page: Page, name: string) {
  const dir = process.env.AX_E2E_SCREENSHOTS;
  if (dir) await page.screenshot({ path: `${dir}/${name}.png` });
}

async function fillConnection(page: Page, url: string, key: string, name = '') {
  await page.getByLabel('Address').fill(url);
  await page.getByRole('textbox', { name: 'API key' }).fill(key);
  await page.getByLabel(/^Name/).fill(name);
}

test('A1: first start shows only the connect screen', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('connect-screen')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Connect to an Ax environment' })).toBeVisible();
  await shot(page, 'ax-admin-connect');
  await expect(nav(page)).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Connections' })).toHaveCount(0);
  // A deep link does not get past it either.
  await page.goto('/#/operations');
  await expect(page.getByTestId('connect-screen')).toBeVisible();
  await expect(nav(page)).toHaveCount(0);
});

test('A2, A4: an address and accepted credentials are required', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Connect', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('Enter the address of the Ax environment.');
  await page.getByLabel('Address').fill(ALPHA);
  await page.getByRole('button', { name: 'Connect', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('Enter the API key for this environment.');
  await page.getByRole('textbox', { name: 'API key' }).fill('not-the-key');
  await page.getByRole('button', { name: 'Connect', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('The server rejected these credentials.');
  await expect(page.getByTestId('connect-screen')).toBeVisible();
  await page.reload();
  await expect(page.getByTestId('connect-screen')).toBeVisible();
});

test('A3: a valid address and key open the application', async ({ page }) => {
  await page.goto('/');
  await fillConnection(page, ALPHA, CONTEXT_KEY, 'Alpha read-only');
  await page.getByRole('button', { name: 'Test', exact: true }).click();
  await expect(page.getByRole('status')).toContainText(/Connected to .* with \d+ tools\./);
  await page.getByRole('button', { name: 'Connect', exact: true }).click();
  await expect(nav(page)).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Status', level: 1 })).toBeVisible();
  await expect(page.getByTestId('tool-result').first()).toContainText(/files/i);
  await expect(page.getByTestId('access-level')).toHaveText('read access');
  // E9: no management pages without the admin grant.
  for (const name of ['Operations', 'Files', 'Usage', 'Logging']) {
    await expect(nav(page).getByRole('link', { name, exact: true })).toHaveCount(0);
  }
  await expect(nav(page).getByRole('link', { name: 'Tools', exact: true })).toBeVisible();
  // No key is shown or stored in the page.
  expect(await page.content()).not.toContain(CONTEXT_KEY);
  expect(await page.evaluate(() => JSON.stringify({ ...localStorage, ...sessionStorage }))).not.toContain(CONTEXT_KEY);
});

test('A8: a reload opens the saved environment without asking again', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Status', level: 1 })).toBeVisible();
  await expect(page.getByTestId('connect-screen')).toHaveCount(0);
});

test('search and the generic tool page work against the environment', async ({ page }) => {
  await page.goto('/#/search');
  await page.getByRole('searchbox', { name: 'Symbol search' }).fill('remote_fixture');
  await page.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(page.getByTestId('tool-result')).toContainText('remote_fixture');

  await page.goto('/#/tools');
  await page.getByRole('option', { name: /^ax_status/ }).click();
  await page.getByRole('form', { name: 'ax_status' }).getByRole('button', { name: 'Run' }).click();
  await expect(page.getByTestId('tool-result')).toContainText(/files/i);
});

test('A6, E9: a second environment is added through the Connections button and switching changes the pages', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Connections' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('option')).toHaveCount(1);
  await dialog.getByRole('button', { name: 'Add connection' }).click();
  await fillConnection(page, ALPHA, ADMIN_KEY, 'Alpha admin');
  await page.getByRole('button', { name: 'Save and connect' }).click();
  await expect(page.getByRole('dialog').getByRole('option')).toHaveCount(2);
  await expect(page.getByRole('dialog').getByRole('option', { name: /Alpha admin/ })).toContainText('active');
  await page.getByRole('dialog').getByRole('button', { name: 'Close' }).last().click();

  await expect(page.getByTestId('access-level')).toHaveText('admin access');
  for (const name of ['Operations', 'Files', 'Usage', 'Logging']) {
    await expect(nav(page).getByRole('link', { name, exact: true })).toBeVisible();
  }

  // A third environment: another project on the same server.
  await page.getByRole('button', { name: 'Connections' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Add connection' }).click();
  await fillConnection(page, BETA, BETA_KEY);
  await page.getByRole('button', { name: 'Save and connect' }).click();
  await expect(page.getByRole('dialog').getByRole('option')).toHaveCount(3);
  await page.getByRole('dialog').getByRole('button', { name: 'Close' }).last().click();
  await expect(page.getByTestId('access-level')).toHaveText('read access');
  await expect(nav(page).getByRole('link', { name: 'Operations', exact: true })).toHaveCount(0);

  // Switch back to the admin connection.
  await page.getByRole('button', { name: 'Connections' }).click();
  await page.getByRole('dialog').getByRole('option', { name: /Alpha admin/ }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Switch to this' }).click();
  await expect(page.getByRole('dialog').getByRole('status')).toHaveText('Switched to Alpha admin.');
  await shot(page, 'ax-admin-connections');
  await page.getByRole('dialog').getByRole('button', { name: 'Close' }).last().click();
  await expect(page.getByTestId('access-level')).toHaveText('admin access');
});

test('E6, E9: management pages run against the environment, and a rebuild asks first', async ({ page }) => {
  await page.goto('/#/operations');
  const sync = page.getByRole('region', { name: 'Sync' });
  await sync.getByRole('button', { name: 'Sync' }).click();
  await expect(sync.getByTestId('tool-result')).toContainText(/Synced \d+ file/);
  await shot(page, 'ax-admin-operations');

  const rebuild = page.getByRole('region', { name: 'Rebuild index' });
  await rebuild.getByRole('button', { name: 'Rebuild' }).click();
  await expect(rebuild.getByRole('alertdialog')).toContainText('Rebuilding clears the index first');
  await expect(rebuild.getByTestId('tool-result')).toHaveCount(0);
  await rebuild.getByRole('button', { name: 'Cancel' }).click();
  await expect(rebuild.getByTestId('tool-result')).toHaveCount(0);
  await rebuild.getByRole('button', { name: 'Rebuild' }).click();
  await rebuild.getByRole('button', { name: 'Yes, rebuild' }).click();
  await expect(rebuild.getByTestId('tool-result')).toBeVisible({ timeout: 120_000 });

  await page.goto('/#/logging');
  const table = page.getByRole('table', { name: 'MCP calls' });
  await expect(table).toBeVisible();
  await expect(table.getByRole('row').filter({ hasText: 'ax_sync' }).first()).toBeVisible();

  await page.goto('/#/files');
  await expect(page.getByTestId('tool-result')).toContainText('lib.rs');
});

test('A7: removing connections deletes them and the last removal returns to the connect screen', async ({ page }) => {
  await page.goto('/');
  for (const remaining of [2, 1]) {
    await page.getByRole('button', { name: 'Connections' }).click();
    const dialog = page.getByRole('dialog');
    await dialog.getByRole('option').first().click();
    await dialog.getByRole('button', { name: 'Remove', exact: true }).click();
    await expect(dialog.getByRole('alertdialog')).toContainText('Its saved credential is deleted');
    await dialog.getByRole('alertdialog').getByRole('button', { name: 'Remove', exact: true }).click();
    await expect(dialog.getByRole('option')).toHaveCount(remaining);
    await dialog.getByRole('button', { name: 'Close' }).last().click();
    await expect(nav(page)).toBeVisible();
  }
  await page.getByRole('button', { name: 'Connections' }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('option').first().click();
  await dialog.getByRole('button', { name: 'Remove', exact: true }).click();
  await dialog.getByRole('alertdialog').getByRole('button', { name: 'Remove', exact: true }).click();
  await expect(page.getByTestId('connect-screen')).toBeVisible();
  await expect(nav(page)).toHaveCount(0);
});
