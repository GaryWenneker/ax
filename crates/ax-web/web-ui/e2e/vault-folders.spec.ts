import { expect, test } from '@playwright/test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

test.describe('Vault folders', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop settings');
  const stamp = Date.now();
  const name = `e2e ${stamp}`;
  const word = `quokka${stamp}`;
  let dir = '';

  test.beforeAll(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), 'ax-vault-folder-'));
    fs.writeFileSync(path.join(dir, 'zebra.md'), `# Zebra ${word}\n\nThe ${word} note.`);
  });

  test.afterAll(async ({ request }) => {
    await request.delete(`/api/vault/folders/${encodeURIComponent(name)}`);
    if (dir) fs.rmSync(dir, { recursive: true, force: true });
  });

  test('Choose… fills the path and name from the folder dialog', async ({ page }) => {
    await page.route('**/api/vault/folder-picker', (route) =>
      route.fulfill({ json: { path: '/Users/me/Work Notes' } }),
    );
    await page.goto('/settings');
    await page.getByRole('button', { name: 'Add folder' }).click();
    const modal = page.getByRole('dialog', { name: 'Add vault folder' });
    await modal.getByRole('button', { name: 'Choose…' }).click();
    await expect(modal.getByLabel('Folder path')).toHaveValue('/Users/me/Work Notes');
    await expect(modal.getByLabel('Folder name')).toHaveValue('Work Notes');

    await page.unroute('**/api/vault/folder-picker');
    await page.route('**/api/vault/folder-picker', (route) =>
      route.fulfill({ status: 501, json: { error: 'no dialog' } }),
    );
    await modal.getByRole('button', { name: 'Choose…' }).click();
    await expect(modal.getByRole('alert')).toHaveText('No folder dialog on this system — type the path.');
    await expect(modal.getByLabel('Folder path')).toHaveValue('/Users/me/Work Notes');
  });

  test('add a folder, index it, see it in the drive and in recall, then remove it', async ({ page, request }) => {
    await page.goto('/settings');
    await page.getByRole('button', { name: 'Add folder' }).click();
    const modal = page.getByRole('dialog', { name: 'Add vault folder' });
    await modal.getByLabel('Folder name').fill(name);
    await modal.getByLabel('Folder path').fill(path.join(dir, 'missing'));
    await modal.getByRole('button', { name: 'Add', exact: true }).click();
    const alert = modal.getByRole('alert');
    await expect(alert).not.toBeEmpty();
    await expect(modal.getByLabel('Folder path')).toHaveAccessibleDescription(/absolute path.*\S/);
    await modal.getByLabel('Folder path').fill(dir);
    await modal.getByRole('button', { name: 'Add', exact: true }).click();
    await expect(modal).toHaveCount(0);

    const row = page.locator(`.vault-folder-row[data-name="${name}"]`);
    await expect(row).toContainText('Synced: 1 added, 0 updated, 0 removed, 0 skipped');
    await expect(row.getByRole('switch', { name: `Index ${name} into memory` })).toHaveAttribute('aria-checked', 'true');

    const file = await request.get(`/dav/folders/${encodeURIComponent(name)}/zebra.md`);
    expect(file.status()).toBe(200);
    expect(await file.text()).toContain(word);

    const recall = await request.get(`/api/memory/recall?q=${word}`);
    expect(recall.ok()).toBeTruthy();
    expect(JSON.stringify(await recall.json())).toContain(`Zebra ${word}`);

    await row.getByRole('switch', { name: `Index ${name} into memory` }).click();
    await expect(row).toContainText('Not indexed');
    const after = await request.get(`/api/memory/recall?q=${word}`);
    expect(JSON.stringify(await after.json())).not.toContain(`Zebra ${word}`);

    await row.getByRole('button', { name: `Remove ${name}` }).click();
    await expect(row).toHaveCount(0);
  });
});
