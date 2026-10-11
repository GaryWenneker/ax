import { expect, test, type Page } from '@playwright/test';

if (process.env.AX_CHROME_EXECUTABLE) {
  test.use({ launchOptions: { executablePath: process.env.AX_CHROME_EXECUTABLE } });
}

async function accessFixture(page: Page, options: { readonly?: boolean; failScopes?: boolean; role?: string } = {}) {
  const calls: { path: string; method: string; body: any }[] = [];
  const members = [
    { id: 'owner', display_name: 'Local owner', email: null, role: 'owner', status: 'active', created_at: '2026-10-10 09:00:00' },
    { id: 'member', display_name: 'Demo developer', email: 'developer@example.com', role: options.role ?? 'member', status: 'active', created_at: '2026-10-10 09:00:00', last_active_at: '2026-10-11T11:30:00+02:00' },
  ];
  const clients = [{ id: 'cli', member_id: 'member', display_name: 'Development CLI', client_type: 'cli', status: 'active', scopes: ['memory:read', 'memory:write', 'context:read'], created_at: '2026-10-10 10:00:00' }];
  const nodes = [{ id: 'replica', display_name: 'Office replica', role: 'replica', trust_state: 'trusted', endpoint: 'https://replica.example.com', last_seen_at: new Date().toISOString(), last_sync_at: '2026-10-11 12:00:00' }];
  await page.route('**/api/**', async route => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    const method = request.method();
    const body = request.postData() ? request.postDataJSON() : null;
    calls.push({ path, method, body });
    let data: unknown;
    if (method !== 'GET') {
      data = { updated: true };
      expect(request.headers()['x-ax-access-csrf']).toBe('fixture-csrf');
      if (path.endsWith('/scopes') && options.failScopes) {
        await route.fulfill({ status: 400, json: { error: 'Permissions were not saved' } });
        return;
      }
      if (path.endsWith('/scopes')) clients[0].scopes = body.scopes;
      if (path.endsWith('/token')) data = { credential: { token: 'axc_documentation_example_not_a_real_credential' } };
      if (path.endsWith('/revoke') && path.includes('/nodes/')) nodes[0].trust_state = 'revoked';
      if (path === '/api/access/team') members.push({ id: 'new-member', status: 'active', created_at: '2026-10-11 12:00:00', ...body });
      if (path === '/api/access/clients') clients.push({ id: 'new-client', status: 'active', created_at: '2026-10-11 12:00:00', ...body });
    } else if (path === '/api/share/status') data = { readonly: options.readonly ?? false, sharing: false, port: 7070 };
    else if (path === '/api/access/session') data = { csrf: 'fixture-csrf' };
    else if (path === '/api/access/team') data = { members };
    else if (path === '/api/access/clients') data = { clients, credentials: [{ id: 'credential', client_id: 'cli', token_prefix: 'axc_demo', created_at: '2026-10-10 10:00:00' }] };
    else if (path === '/api/access/nodes') data = { nodes, local: { node_id: 'primary-local', role: 'primary', sync_state: 'running' } };
    else if (path === '/api/ship/config') data = { config: { ship: { target_branch: 'main', web_port: 7070 }, remote: { provider: 'github' }, quality_gate: { steps: [], tests: { runner: 'cargo test' } }, sonar: {}, ui: {} } };
    else if (path === '/api/stats') data = { readonly: options.readonly ?? false, project_name: 'Demo project', node_count: 0, edge_count: 0, file_count: 0, db_size_bytes: 0, languages: [] };
    else if (path === '/api/ship/review-language') data = { code: 'en', languages: [] };
    await route.fulfill({ status: data === undefined ? 404 : 200, json: data ?? { error: 'Fixture endpoint not provided' } });
  });
  await page.addInitScript(() => { localStorage.setItem('ax-theme', 'azure-light'); localStorage.setItem('ax-theme-mint-v2', '1'); });
  await page.emulateMedia({ reducedMotion: 'reduce' });
  return { calls, members, clients, nodes };
}

test('only the selected access panel loads and direct links survive reload', async ({ page }) => {
  const { calls } = await accessFixture(page);
  await page.goto('/settings');
  await expect(page.getByRole('heading', { name: 'Appearance', exact: true })).toBeVisible();
  expect(calls.some(call => call.path.startsWith('/api/access/'))).toBe(false);
  await page.getByRole('navigation', { name: 'Settings navigation' }).getByRole('button', { name: 'Team', exact: true }).click();
  await expect(page.getByLabel('Role for Demo developer')).toBeVisible();
  expect(calls.some(call => call.path === '/api/access/nodes' || call.path === '/api/access/clients')).toBe(false);
  await expect(page.getByLabel('Role for Local owner')).toBeDisabled();
  await expect(page).toHaveURL(/section=team/);
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Team', exact: true })).toBeVisible();
  await page.getByLabel('Filter settings').fill('access');
  await expect(page.getByRole('navigation', { name: 'Settings navigation' }).getByRole('button')).toHaveCount(3);
});

test('a failed permission save preserves the draft and role restrictions', async ({ page }) => {
  const { calls } = await accessFixture(page, { failScopes: true, role: 'readonly' });
  await page.goto('/settings?section=clients');
  await expect(page.getByLabel('memory:write', { exact: true })).toBeDisabled();
  await expect(page.getByLabel('memory:write', { exact: true })).not.toBeChecked();
  await expect(page.getByLabel('admin:team', { exact: true })).toBeDisabled();
  await page.getByLabel('preflight:read', { exact: true }).check();
  await page.getByRole('button', { name: 'Save scopes' }).click();
  await expect(page.getByRole('alert')).toHaveText('Permissions were not saved');
  await expect(page.getByLabel('preflight:read', { exact: true })).toBeChecked();
  await expect(page.getByRole('button', { name: 'Discard changes' })).toBeEnabled();
  expect(calls.find(call => call.path.endsWith('/scopes'))?.body.scopes).toEqual(['memory:read', 'context:read', 'preflight:read']);
});

test('member and client forms send email and role-limited defaults', async ({ page }) => {
  const { calls, members } = await accessFixture(page);
  await page.goto('/settings?section=team');
  await page.getByRole('button', { name: 'Add member', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Name', { exact: true }).fill('Read-only colleague');
  await dialog.getByLabel('Email (optional)').fill('colleague@example.com');
  await dialog.getByLabel('Role', { exact: true }).selectOption('readonly');
  await dialog.getByRole('button', { name: 'Add member', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(members.find(member => member.display_name === 'Read-only colleague')?.email).toBe('colleague@example.com');
  await page.getByRole('navigation', { name: 'Settings navigation' }).getByRole('button', { name: 'Clients & tokens', exact: true }).click();
  await page.getByRole('button', { name: 'Add client', exact: true }).click();
  await dialog.getByLabel('Client name').fill('Read-only integration');
  await dialog.getByLabel('Application type').selectOption('custom');
  await dialog.getByLabel('Team member').selectOption('new-member');
  await dialog.getByRole('button', { name: 'Add client', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(calls.find(call => call.path === '/api/access/clients' && call.method === 'POST')?.body.scopes).toEqual(['memory:read', 'preflight:read', 'context:read']);
});

test('read-only sessions disable writes and tokens vanish on navigation', async ({ page }) => {
  const { calls } = await accessFixture(page, { readonly: true });
  await page.goto('/settings?section=clients');
  await expect(page.getByRole('button', { name: 'Issue token', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Revoke client' })).toBeDisabled();
  expect(calls.some(call => call.method !== 'GET')).toBe(false);
});

test('token copying and node blocking use the existing API and can be canceled', async ({ page }) => {
  const { calls } = await accessFixture(page);
  await page.goto('/settings?section=clients');
  await page.evaluate(() => { Object.defineProperty(navigator, 'clipboard', { value: { writeText: async (value: string) => { (window as any).copiedToken = value; } }, configurable: true }); });
  await page.getByRole('button', { name: 'Issue token', exact: true }).click();
  await page.getByRole('button', { name: 'Copy token', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Copied', exact: true })).toBeVisible();
  expect(await page.evaluate(() => (window as any).copiedToken)).toBe('axc_documentation_example_not_a_real_credential');
  await page.getByRole('navigation', { name: 'Settings navigation' }).getByRole('button', { name: 'Replication', exact: true }).click();
  await expect(page.getByText('axc_documentation_example_not_a_real_credential', { exact: true })).toHaveCount(0);
  await expect(page.getByText('Local Primary', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Block node', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Cancel' }).click();
  expect(calls.some(call => call.path.endsWith('/nodes/replica/revoke'))).toBe(false);
  await page.getByRole('button', { name: 'Block node', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Block node', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Allow re-enrollment' })).toBeVisible();
});

test('access panels fit light, dark and mobile and refresh documentation screenshots', async ({ page }) => {
  await accessFixture(page);
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  for (const theme of ['azure-light', 'azure-dark']) {
    await page.goto('/settings');
    if (theme === 'azure-dark') await page.getByRole('button', { name: 'Azure Portal Dark', exact: true }).click();
    await page.setViewportSize({ width: 1440, height: 1100 });
    for (const section of ['team', 'clients', 'nodes']) {
      await page.getByRole('navigation', { name: 'Settings navigation' }).locator(`[aria-controls="settings-panel-${section}"]`).click();
      const panel = page.locator(`#settings-panel-${section}`);
      await expect(panel.locator('.access-note').first()).not.toContainText('Loading');
      await expect(panel).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
      if (process.env.AX_ACCESS_SCREENSHOTS) await page.screenshot({ path: `../../../site/public/screenshots/cc-access-${section}${theme === 'azure-dark' ? '-dark' : ''}.png` });
      await page.setViewportSize({ width: 393, height: 851 });
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
      expect(await panel.evaluate(element => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
      await page.setViewportSize({ width: 1440, height: 1100 });
    }
  }
  expect(errors).toEqual([]);
});
