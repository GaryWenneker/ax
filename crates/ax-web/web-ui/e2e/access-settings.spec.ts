import { expect, test } from '@playwright/test';

if (process.env.AX_CHROME_EXECUTABLE) {
  test.use({ launchOptions: { executablePath: process.env.AX_CHROME_EXECUTABLE } });
}

test('access management sends CSRF and shows new tokens only until dismissed', async ({ page }) => {
  const mutations: { path: string; body: unknown }[] = [];
  const member = { id: 'm1', display_name: 'Demo member', role: 'member', status: 'active', created_at: '2026-10-11' };
  const client = { id: 'c1', member_id: 'm1', display_name: 'Demo client', client_type: 'cli', scopes: ['memory:read'], status: 'active', created_at: '2026-10-11' };
  await page.route('**/api/access/**', async route => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    if (request.method() !== 'GET') {
      expect(request.headers()['x-ax-access-csrf']).toBe('access-test-csrf');
      const body = request.postData() ? request.postDataJSON() : null;
      mutations.push({ path, body });
      if (path.endsWith('/token') || path.endsWith('/rotate')) {
        await route.fulfill({ json: { credential: { token: 'axc_test_shown_once' } } });
      } else {
        if (path === '/api/access/team/m1') Object.assign(member, body);
        if (path.endsWith('/scopes')) Object.assign(client, body);
        await route.fulfill({ json: { updated: true } });
      }
    } else if (path.endsWith('/session')) {
      await route.fulfill({ json: { csrf: 'access-test-csrf' } });
    } else if (path.endsWith('/team')) {
      await route.fulfill({ json: { members: [member] } });
    } else if (path.endsWith('/clients')) {
      await route.fulfill({ json: { clients: [client], credentials: [{ id: 'cred1', client_id: 'c1', token_prefix: 'axc_prefix', created_at: '2026-10-11' }] } });
    } else {
      await route.fulfill({ json: { nodes: [], local: { node_id: 'demo-replica', role: 'replica', primary_url: 'https://primary.example', sync_state: 'synced' } } });
    }
  });
  await page.goto('/settings');
  // These controls also work after integration with the portal Settings panels.
  const teamNav = page.locator('.settings-navigation-item[aria-controls="settings-panel-team"]');
  if (await teamNav.count()) await teamNav.click();
  const team = page.locator('.settings-card').filter({ has: page.getByRole('heading', { name: 'Team', exact: true }) });
  await team.getByLabel('Role for Demo member').selectOption('readonly');
  await expect.poll(() => member.role).toBe('readonly');
  const clientsNav = page.locator('.settings-navigation-item[aria-controls="settings-panel-clients"]');
  if (await clientsNav.count()) await clientsNav.click();
  const clients = page.locator('.settings-card').filter({ has: page.getByRole('heading', { name: 'Clients & tokens', exact: true }) });
  await clients.getByRole('button', { name: 'Issue token', exact: true }).click();
  await expect(page.getByText('axc_test_shown_once', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Dismiss', exact: true }).click();
  await expect(page.getByText('axc_test_shown_once', { exact: true })).toHaveCount(0);
  await clients.getByLabel('context:read', { exact: true }).check();
  await clients.getByRole('button', { name: 'Save scopes' }).click();
  await expect.poll(() => client.scopes).toContain('context:read');
  await clients.getByRole('button', { name: 'Rotate tokens' }).click();
  await expect(page.getByText('axc_test_shown_once', { exact: true })).toBeVisible();
  await clients.getByRole('button', { name: 'Revoke token', exact: true }).click();
  await expect.poll(() => mutations.some(m => m.path === '/api/access/credentials/cred1/revoke')).toBe(true);
  expect(mutations.some(m => m.path === '/api/access/clients/c1/rotate')).toBe(true);
});
