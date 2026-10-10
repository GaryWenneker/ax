import { expect, test } from '@playwright/test';

// Optional installed renderer for environments without Playwright's bundled browser.
if (process.env.AX_CHROME_EXECUTABLE) {
  test.use({ launchOptions: { executablePath: process.env.AX_CHROME_EXECUTABLE } });
}

test('remote Settings sends CSRF, shows the browser fallback, and disconnects saved credentials', async ({ page, context }) => {
  let connected = false;
  let polls = 0;
  let loginBody: unknown;
  let csrfHeader: string | undefined;
  await context.route('https://issuer.example/**', route => route.fulfill({ body: 'Provider sign-in' }));
  await page.route('**/api/remote-mcp/**', async route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/login')) {
      loginBody = route.request().postDataJSON();
      csrfHeader = route.request().headers()['x-ax-csrf'];
      await route.fulfill({ json: { id: 'attempt', authorization_url: 'https://issuer.example/authorize' } });
    } else if (path.endsWith('/login/attempt')) {
      connected = ++polls > 1;
      await route.fulfill({ json: { id: 'attempt', state: connected ? 'connected' : 'pending' } });
    } else if (path.endsWith('/logout')) {
      expect(route.request().headers()['x-ax-csrf']).toBe('test-csrf');
      connected = false;
      await route.fulfill({ json: { ok: true } });
    } else {
      await route.fulfill({ json: { csrf: 'test-csrf', connections: connected ? [{ url: 'https://ax.example/projects/demo/mcp', client_id: 'native', issuer: 'https://issuer.example/', state: 'stored', expires_at: 9999999999 }] : [] } });
    }
  });
  await page.addInitScript(() => { window.open = () => null; });
  await page.goto('/settings');
  const section = page.locator('.remote-mcp-settings');
  await expect(section.getByRole('heading', { name: 'Remote Ax connections' })).toBeVisible();
  await section.getByLabel('Project MCP URL').fill('https://ax.example/projects/demo/mcp');
  await section.getByLabel('OAuth client ID').fill('native');
  await section.getByRole('button', { name: 'Connect with browser' }).click();
  await expect(section.getByRole('link', { name: 'Open sign-in if your browser did not open' })).toHaveAttribute('href', 'https://issuer.example/authorize');
  await expect(section.getByRole('status')).toContainText('verified access');
  expect(loginBody).toEqual({ url: 'https://ax.example/projects/demo/mcp', client_id: 'native' });
  expect(csrfHeader).toBe('test-csrf');
  await expect(section).toContainText('Credentials saved');
  await section.getByRole('button', { name: 'Disconnect' }).click();
  await expect(section.getByRole('status')).toContainText('Local credentials removed');
  await expect(section.getByRole('button', { name: 'Disconnect' })).toHaveCount(0);
});

test('remote Settings shows a denied login without claiming a connection', async ({ page }) => {
  await page.route('**/api/remote-mcp/**', route => {
    const path = new URL(route.request().url()).pathname;
    return path.endsWith('/login')
      ? route.fulfill({ status: 400, json: { error: 'Project access denied' } })
      : route.fulfill({ json: { csrf: 'test-csrf', connections: [] } });
  });
  await page.goto('/settings');
  const section = page.locator('.remote-mcp-settings');
  await section.getByLabel('Project MCP URL').fill('https://ax.example/projects/demo/mcp');
  await section.getByLabel('OAuth client ID').fill('native');
  await section.getByRole('button', { name: 'Connect with browser' }).click();
  await expect(section.getByRole('alert')).toHaveText('Project access denied');
  await expect(section.getByRole('button', { name: 'Connect with browser' })).toBeEnabled();
  await expect(section.getByRole('status')).toHaveCount(0);
});

test('remote Settings cancels a pending browser login with CSRF', async ({ page }) => {
  let cancelled = false;
  await page.route('**/api/remote-mcp/**', async route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/cancel')) {
      expect(route.request().headers()['x-ax-csrf']).toBe('test-csrf');
      cancelled = true;
      await route.fulfill({ json: { ok: true } });
    } else if (path.endsWith('/login')) {
      await route.fulfill({ json: { id: 'attempt', authorization_url: 'https://issuer.example/authorize' } });
    } else if (path.endsWith('/login/attempt')) {
      await route.fulfill({ json: { id: 'attempt', state: 'pending' } });
    } else {
      await route.fulfill({ json: { csrf: 'test-csrf', connections: [] } });
    }
  });
  await page.addInitScript(() => { window.open = () => null; });
  await page.goto('/settings');
  const section = page.locator('.remote-mcp-settings');
  await section.getByLabel('Project MCP URL').fill('https://ax.example/projects/demo/mcp');
  await section.getByLabel('OAuth client ID').fill('native');
  await section.getByRole('button', { name: 'Connect with browser' }).click();
  await section.getByRole('button', { name: 'Cancel sign-in' }).click();
  await expect(section.getByRole('status')).toHaveText('Sign-in cancelled.');
  await expect(section.getByRole('button', { name: 'Connect with browser' })).toBeEnabled();
  expect(cancelled).toBe(true);
});
