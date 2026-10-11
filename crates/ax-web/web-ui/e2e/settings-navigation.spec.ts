import { expect, test } from '@playwright/test';

test('Settings contextual navigation jumps between grouped sections', async ({ page }) => {
  await page.goto('/settings');

  const nav = page.getByRole('navigation', { name: 'Settings sections' });
  await expect(nav).toBeVisible();
  await expect(nav.getByText('Integrations')).toBeVisible();
  await expect(nav.getByText('Workspace')).toBeVisible();
  await expect(nav.getByText('Intelligence')).toBeVisible();
  await expect(nav.getByText('Command Center', { exact: true }).first()).toBeVisible();

  const plugins = nav.getByRole('button', { name: 'Plugins' });
  await plugins.click();
  await expect(page).toHaveURL(/#settings-plugins$/);
  await expect(plugins).toHaveAttribute('aria-current', 'location');
  await expect(page.locator('#settings-plugins')).toBeInViewport();

  const vault = nav.getByRole('button', { name: 'Vault connection' });
  await vault.click();
  await expect(page).toHaveURL(/#settings-vault$/);
  await expect(page.locator('#settings-vault')).toBeInViewport();
});

test('Settings direct hash links land on the requested section', async ({ page }) => {
  await page.goto('/settings#settings-embeddings');
  await expect(page.locator('#settings-embeddings')).toBeInViewport();
  await expect(
    page.getByRole('navigation', { name: 'Settings sections' }).getByRole('button', { name: 'Embeddings' }),
  ).toHaveAttribute('aria-current', 'location');
});

test('Settings uses a compact section selector on narrow screens', async ({ page }) => {
  await page.setViewportSize({ width: 700, height: 800 });
  await page.goto('/settings');

  await expect(page.getByRole('navigation', { name: 'Settings sections' })).toBeHidden();
  const selector = page.getByLabel('Settings section', { exact: true });
  await expect(selector).toBeVisible();
  await selector.selectOption('settings-okf');
  await expect(page).toHaveURL(/#settings-okf$/);
  await expect(page.locator('#settings-okf')).toBeInViewport();
});
