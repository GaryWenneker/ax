import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { expect, test, type Locator, type Page } from '@playwright/test';

// One story, in order, against one running ax-admin and a real headless Ax:
// first start, connecting, several environments, management, and removing
// everything again. Started by tools/ax-admin-e2e.sh.
test.describe.configure({ mode: 'serial' });

const env = (name: string): string => {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is not set; run tools/ax-admin-e2e.sh`);
  return value;
};
const ALPHA = env('AX_E2E_ALPHA_URL');
const BETA = env('AX_E2E_BETA_URL');
const ALPHA_DIR = env('AX_E2E_ALPHA_DIR');
const CONTEXT_KEY = env('AX_E2E_CONTEXT_KEY');
const ADMIN_KEY = env('AX_E2E_ADMIN_KEY');
const BETA_KEY = env('AX_E2E_BETA_KEY');

const nav = (page: Page) => page.getByRole('navigation', { name: 'Main navigation' });
const connections = (page: Page) => page.getByRole('dialog', { name: 'Connections' });
const okResult = (scope: Page | Locator) => scope.locator('[data-testid="tool-result"][data-outcome="ok"]');

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

async function addConnection(page: Page, url: string, key: string, name = '') {
  await page.getByRole('button', { name: 'Connections' }).click();
  await connections(page).getByRole('button', { name: 'Add connection' }).click();
  // The form takes the focus when the panel switches to it.
  await expect(page.getByLabel('Address')).toBeFocused();
  await fillConnection(page, url, key, name);
  await page.getByRole('button', { name: 'Save and connect' }).click();
  // Back in the list, the selected connection has the focus.
  await expect(connections(page).getByRole('option', { selected: true })).toBeFocused();
}

async function search(page: Page, query: string) {
  await page.goto('/#/search');
  await page.getByRole('searchbox', { name: 'Symbol search' }).fill(query);
  await page.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(okResult(page)).toBeVisible();
  return okResult(page);
}

/** How often each tool was called on the environment, from its own call log. */
async function callCount(page: Page, tool: string): Promise<number> {
  await page.goto('/#/logging');
  await page.getByRole('searchbox', { name: 'Filter by tool' }).fill(tool);
  await page.getByRole('button', { name: 'Show', exact: true }).click();
  await expect(page.getByText(`The latest calls of ${tool}.`)).toBeVisible();
  await expect(page.getByTestId('call-log').or(page.getByText('No calls recorded'))).toBeVisible();
  return page.getByTestId('call-log').locator('.page-item').count();
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

test('A2, A4, A5: an address and accepted credentials are required', async ({ page }) => {
  await page.goto('/');
  const connect = page.getByRole('button', { name: 'Connect', exact: true });
  await connect.click();
  await expect(page.getByRole('alert')).toHaveText('Enter the address of the Ax environment.');
  await expect(page.getByLabel('Address')).toHaveAttribute('aria-invalid', 'true');
  await page.getByLabel('Address').fill(ALPHA);
  await connect.click();
  await expect(page.getByRole('alert')).toHaveText('Enter the API key for this environment.');
  await expect(page.getByRole('textbox', { name: 'API key' })).toHaveAttribute('aria-invalid', 'true');
  await page.getByRole('textbox', { name: 'API key' }).fill('not-the-key');
  await connect.click();
  await expect(page.getByRole('alert')).toHaveText('The server rejected these credentials.');
  // A5: nothing listens at this address.
  await page.getByLabel('Address').fill('http://127.0.0.1:9/projects/alpha/mcp');
  await connect.click();
  await expect(page.getByRole('alert')).toHaveText('The server could not be reached. Check the address and your network.');
  // (A page that is not Ax, and a redirect, are covered by the Rust tests,
  // which can stand up such servers.)
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
  await expect(okResult(page).first()).toContainText(/files/i);
  await expect(page.getByTestId('active-name')).toHaveText('Alpha read-only');
  await expect(page.getByTestId('access-level')).toHaveText('read access');
  // E9: no management pages without the admin grant.
  for (const name of ['Operations', 'Files', 'Usage', 'Logging']) {
    await expect(nav(page).getByRole('link', { name, exact: true })).toHaveCount(0);
  }
  await expect(nav(page).getByRole('link', { name: 'Tools', exact: true })).toBeVisible();
  // The Connections panel does not open by itself after the first connect.
  await expect(connections(page)).toHaveCount(0);
  // No key is shown or stored in the page.
  expect(await page.content()).not.toContain(CONTEXT_KEY);
  expect(await page.evaluate(() => JSON.stringify({ ...localStorage, ...sessionStorage }))).not.toContain(CONTEXT_KEY);
});

test('a page reload opens the saved environment without asking again', async ({ page }) => {
  // A8 proper (a process restart) is covered by the Rust tests; keys are kept
  // in memory in this run and would not survive a restart.
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Status', level: 1 })).toBeVisible();
  await expect(page.getByTestId('connect-screen')).toHaveCount(0);
});

test('search, the generic tool page and keyboard use of lists and tabs', async ({ page }) => {
  await expect(await search(page, 'remote_fixture')).toContainText('remote_fixture');

  await page.goto('/#/tools');
  const list = page.getByRole('listbox', { name: 'Tools' });
  // One tab stop; the arrow keys move the selection.
  await expect(list.locator('[tabindex="0"]')).toHaveCount(1);
  const first = list.getByRole('option').first();
  await first.focus();
  await page.keyboard.press('ArrowDown');
  await expect(list.getByRole('option').nth(1)).toHaveAttribute('aria-selected', 'true');
  await expect(list.getByRole('option').nth(1)).toBeFocused();
  // The selected row has a highlight and a border in the accent text colour.
  const style = await list.getByRole('option').nth(1).evaluate((row) => {
    const css = getComputedStyle(row);
    const probe = document.createElement('i');
    probe.style.color = 'var(--accent-text)';
    row.append(probe);
    const accent = getComputedStyle(probe).color;
    probe.remove();
    return { border: css.borderTopColor, width: css.borderTopWidth, background: css.backgroundColor, accent };
  });
  expect(style.width).toBe('1px');
  expect(style.border).toBe(style.accent);
  expect(style.background).not.toBe('rgba(0, 0, 0, 0)');

  await list.getByRole('option', { name: /^ax_status/ }).click();
  await page.getByRole('form', { name: 'ax_status' }).getByRole('button', { name: 'Run' }).click();
  await expect(okResult(page)).toContainText(/files/i);

  await page.goto('/#/nodes');
  const tabs = page.getByRole('tablist', { name: 'View' });
  await tabs.getByRole('tab', { name: 'Source' }).focus();
  // An arrow key moves the focus; it does not choose the tab (choosing asks the server).
  await page.keyboard.press('ArrowRight');
  await expect(tabs.getByRole('tab', { name: 'Callers' })).toBeFocused();
  await expect(tabs.getByRole('tab', { name: 'Source' })).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('Enter');
  await expect(tabs.getByRole('tab', { name: 'Callers' })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', 'node-tab-ax_callers');
  await expect(page.getByRole('tabpanel')).toHaveAttribute('tabindex', '0');
});

test('A6, E9: more environments are added through the Connections button and switching changes what answers', async ({ page }) => {
  await page.goto('/');
  await addConnection(page, ALPHA, ADMIN_KEY, 'Alpha admin');
  await expect(connections(page).getByRole('option')).toHaveCount(2);
  await expect(connections(page).getByRole('option', { name: /Alpha admin/ })).toContainText('active');
  await connections(page).getByRole('button', { name: 'Done' }).click();

  await expect(page.getByTestId('active-name')).toHaveText('Alpha admin');
  await expect(page.getByTestId('access-level')).toHaveText('admin access');
  for (const name of ['Operations', 'Files', 'Usage', 'Logging']) {
    await expect(nav(page).getByRole('link', { name, exact: true })).toBeVisible();
  }

  // A third environment: another project on the same server.
  await addConnection(page, BETA, BETA_KEY);
  await expect(connections(page).getByRole('option')).toHaveCount(3);
  await connections(page).getByRole('button', { name: 'Done' }).click();
  await expect(page.getByTestId('access-level')).toHaveText('read access');
  await expect(nav(page).getByRole('link', { name: 'Operations', exact: true })).toHaveCount(0);

  // Switch back to the admin connection.
  await page.getByRole('button', { name: 'Connections' }).click();
  await connections(page).getByRole('option', { name: /Alpha admin/ }).click();
  await connections(page).getByRole('button', { name: 'Switch to this' }).click();
  await expect(connections(page).getByRole('status')).toHaveText('Switched to Alpha admin.');
  await shot(page, 'ax-admin-connections');
  await connections(page).getByRole('button', { name: 'Done' }).click();
  await expect(page.getByTestId('active-name')).toHaveText('Alpha admin');
  await expect(page.getByTestId('access-level')).toHaveText('admin access');
});

test('a page that still shows another environment runs nothing and says why', async ({ page, context }) => {
  // (The project was indexed once from the command line when it was set up.)
  const rebuildsBefore = await callCount(page, 'ax_index');
  await page.goto('/#/operations');
  await expect(page.getByTestId('active-name')).toHaveText('Alpha admin');
  // Another window switches ax-admin to a different environment.
  const other = await context.newPage();
  await other.goto('/');
  await other.getByRole('button', { name: 'Connections' }).click();
  await connections(other).getByRole('option', { name: /Alpha read-only/ }).click();
  await connections(other).getByRole('button', { name: 'Switch to this' }).click();
  await expect(connections(other).getByRole('status')).toHaveText('Switched to Alpha read-only.');

  // This page still offers to rebuild "Alpha admin".
  await page.getByRole('region', { name: 'Rebuild index' }).getByRole('button', { name: 'Rebuild' }).click();
  await page.getByRole('dialog', { name: 'Rebuild index on Alpha admin?' }).getByRole('button', { name: 'Rebuild on Alpha admin' }).click();
  await expect(page.getByRole('alert')).toContainText('so this page now shows that one. Nothing was run.');
  await expect(page.getByTestId('active-name')).toHaveText('Alpha read-only');
  await expect(nav(page).getByRole('link', { name: 'Operations', exact: true })).toHaveCount(0);

  // Back to the admin connection; the refused rebuild never reached the server.
  await connections(other).getByRole('option', { name: /Alpha admin/ }).click();
  await connections(other).getByRole('button', { name: 'Switch to this' }).click();
  await expect(connections(other).getByRole('status')).toHaveText('Switched to Alpha admin.');
  await other.close();
  await page.reload();
  await expect(page.getByTestId('active-name')).toHaveText('Alpha admin');
  expect(await callCount(page, 'ax_index')).toBe(rebuildsBefore);
});

test('E6, E9: every operation asks first, runs on the environment, and its new code is found there only', async ({ page }) => {
  // New code appears in the alpha project on the server.
  writeFileSync(join(ALPHA_DIR, 'added.rs'), 'pub fn added_while_testing() -> u32 { 7 }\n');
  const syncsBefore = await callCount(page, 'ax_sync');
  const rebuildsBefore = await callCount(page, 'ax_index');

  await page.goto('/#/operations');
  const sync = page.getByRole('region', { name: 'Sync' });
  await sync.getByRole('button', { name: 'Sync' }).click();
  // The question names the environment; Cancel sends nothing.
  const ask = page.getByRole('dialog', { name: 'Sync on Alpha admin?' });
  await expect(ask).toBeVisible();
  await ask.getByRole('button', { name: 'Cancel' }).click();
  await expect(ask).toHaveCount(0);
  await expect(sync.getByRole('button', { name: 'Sync' })).toBeFocused();

  const rebuild = page.getByRole('region', { name: 'Rebuild index' });
  await rebuild.getByRole('button', { name: 'Rebuild' }).click();
  const askRebuild = page.getByRole('dialog', { name: 'Rebuild index on Alpha admin?' });
  await expect(askRebuild).toContainText('Rebuilding clears the index first');
  await page.keyboard.press('Escape');
  await expect(askRebuild).toHaveCount(0);
  // Nothing reached the environment so far.
  expect(await callCount(page, 'ax_sync')).toBe(syncsBefore);
  expect(await callCount(page, 'ax_index')).toBe(rebuildsBefore);
  // (Whether the new file is findable yet is not asserted here: a running Ax
  // watches its projects and may have picked it up by itself.)

  await page.goto('/#/operations');
  await page.getByRole('region', { name: 'Sync' }).getByRole('button', { name: 'Sync' }).click();
  await page.getByRole('dialog', { name: 'Sync on Alpha admin?' }).getByRole('button', { name: 'Sync on Alpha admin' }).click();
  // Focus goes back to the button that asked, also while the operation runs.
  await expect(page.getByRole('region', { name: 'Sync' }).getByRole('button', { name: 'Sync' })).toBeFocused();
  await expect(okResult(page.getByRole('region', { name: 'Sync' }))).toContainText(/Synced \d+ file/);
  await shot(page, 'ax-admin-operations');

  await page.getByRole('region', { name: 'Rebuild index' }).getByRole('button', { name: 'Rebuild' }).click();
  await page.getByRole('dialog', { name: 'Rebuild index on Alpha admin?' }).getByRole('button', { name: 'Rebuild on Alpha admin' }).click();
  await expect(okResult(page.getByRole('region', { name: 'Rebuild index' }))).toBeVisible({ timeout: 120_000 });

  await page.getByRole('region', { name: 'Reload policy' }).getByRole('button', { name: 'Reload' }).click();
  await page.getByRole('dialog', { name: 'Reload policy on Alpha admin?' }).getByRole('button', { name: 'Reload on Alpha admin' }).click();
  await expect(okResult(page.getByRole('region', { name: 'Reload policy' }))).toBeVisible({ timeout: 120_000 });

  // E6: after the sync and the rebuild the new symbol is in the index, and
  // exactly one call of each reached the environment.
  await expect(await search(page, 'added_while_testing')).toContainText('added.rs');
  expect(await callCount(page, 'ax_sync')).toBe(syncsBefore + 1);
  await shot(page, 'ax-admin-logging');
  expect(await callCount(page, 'ax_index')).toBe(rebuildsBefore + 1);

  await page.goto('/#/files');
  await expect(okResult(page)).toContainText('added.rs');

  // The Tools page asks before an index-changing tool as well.
  await page.goto('/#/tools');
  await page.getByRole('option', { name: /^ax_index/ }).click();
  await page.getByRole('form', { name: 'ax_index' }).getByRole('button', { name: 'Run' }).click();
  await expect(page.getByRole('dialog', { name: 'Run ax_index on Alpha admin?' })).toBeVisible();
  await page.keyboard.press('Escape');
  expect(await callCount(page, 'ax_index')).toBe(rebuildsBefore + 1);

  // A6: the other environment answers for itself. Its project has no such file.
  await page.getByRole('button', { name: 'Connections' }).click();
  await connections(page).getByRole('option', { name: /beta/ }).click();
  await connections(page).getByRole('button', { name: 'Switch to this' }).click();
  await connections(page).getByRole('button', { name: 'Done' }).click();
  await expect(page.getByTestId('active-name')).toHaveText('127.0.0.1 / beta');
  await expect((await search(page, 'added_while_testing')).getByText('added.rs')).toHaveCount(0);
  await expect(await search(page, 'remote_fixture')).toContainText('remote_fixture');
});

test('A7: removing connections deletes them, another one becomes active, and the last removal returns to the connect screen', async ({ page }) => {
  await page.goto('/');
  const remove = async (name: RegExp) => {
    await page.getByRole('button', { name: 'Connections' }).click();
    await connections(page).getByRole('option', { name }).click();
    await connections(page).getByRole('button', { name: 'Remove', exact: true }).click();
    const ask = page.getByRole('dialog', { name: /^Remove / });
    await expect(ask).toContainText('Its saved credential is deleted');
    await ask.getByRole('button', { name: 'Remove', exact: true }).click();
  };
  // beta is active; removing it activates the first remaining connection.
  await remove(/beta/);
  await expect(connections(page).getByRole('option')).toHaveCount(2);
  // The focus is on a row that is still there, not on the page body.
  await expect(connections(page).locator('[role="option"][tabindex="0"]')).toBeFocused();
  await expect(connections(page).getByRole('option', { name: /Alpha read-only/ })).toContainText('active');
  await connections(page).getByRole('button', { name: 'Done' }).click();
  await expect(page.getByTestId('active-name')).toHaveText('Alpha read-only');
  await expect(page.getByTestId('access-level')).toHaveText('read access');

  await remove(/Alpha read-only/);
  await expect(connections(page).getByRole('option')).toHaveCount(1);
  await connections(page).getByRole('button', { name: 'Done' }).click();
  await expect(page.getByTestId('active-name')).toHaveText('Alpha admin');

  await remove(/Alpha admin/);
  await expect(page.getByTestId('connect-screen')).toBeVisible();
  await expect(nav(page)).toHaveCount(0);
  await page.reload();
  await expect(page.getByTestId('connect-screen')).toBeVisible();
});
