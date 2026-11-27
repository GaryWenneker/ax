import { expect, test } from '@playwright/test';

// The sign-in gate must keep the app mounted when a background session check
// fails for a transient reason, and only show the sign-in page on a real 401.
test('a failed background session check keeps the app and its state', async ({ page }) => {
  let session: 'ok' | 'down' | 'unauthorized' = 'ok';
  await page.route('**/api/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/web-auth/status') {
      if (session === 'down') return route.abort('connectionrefused');
      return route.fulfill({ json: { enabled: true } });
    }
    if (path === '/api/web-auth/session') {
      if (session === 'unauthorized') return route.fulfill({ status: 401, json: { error: 'no session' } });
      return route.fulfill({ json: { authenticated: true } });
    }
    return route.fulfill({ json: {} });
  });
  await page.goto('/logging');
  const nav = page.getByRole('navigation', { name: 'Main navigation' });
  await expect(nav).toBeVisible();
  // Tag the mounted element: an unmount and remount would produce a new one.
  await nav.evaluate((el) => el.setAttribute('data-e2e-mounted', 'first'));
  const sameApp = page.locator('[data-e2e-mounted="first"]');

  session = 'down';
  await page.evaluate(() => window.dispatchEvent(new Event('ax-web-auth-changed')));
  await expect(page.locator('.auth-offline')).toBeVisible();
  await expect(page.getByText('Authentication unavailable')).toHaveCount(0);
  await expect(sameApp).toHaveCount(1);

  session = 'ok';
  await page.locator('.auth-offline').getByRole('button', { name: 'Retry now' }).click();
  await expect(page.locator('.auth-offline')).toHaveCount(0);
  await expect(sameApp).toHaveCount(1);

  session = 'unauthorized';
  await page.evaluate(() => window.dispatchEvent(new Event('ax-web-auth-changed')));
  await expect(page.getByRole('heading', { name: 'Sign in' })).toBeVisible();
});
