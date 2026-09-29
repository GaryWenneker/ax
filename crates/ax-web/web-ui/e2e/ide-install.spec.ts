import { expect, test, type Route } from '@playwright/test';

// Every /api/agent call is answered here, so the test never changes real IDE configs.
test.describe('IDEs & agents', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop settings');

  test('lists IDEs by state, connects one, and shows what changed', async ({ page }) => {
    let cursorConnected = false;
    const installs: string[][] = [];
    const target = (id: string, display_name: string, detected: boolean, configured: boolean) => ({
      id,
      display_name,
      detected,
      cli_on_path: false,
      cli_installable: false,
      configured,
      config_paths: configured ? [`/home/me/.${id}/mcp.json`] : [],
    });
    const status = () => ({
      ok: true,
      readonly: false,
      targets: [
        target('claude', 'Claude Code', true, true),
        target('cursor', 'Cursor', true, cursorConnected),
        target('zed', 'Zed', false, false),
      ],
      config: { enabled_targets: [], terminal_mode: 'builtin', active_profile: {}, profiles: {} },
      all_targets: ['claude', 'cursor', 'zed'],
    });
    await page.route('**/api/agent/status', (route: Route) => route.fulfill({ json: status() }));
    await page.route('**/api/agent/install', async (route: Route) => {
      const body = route.request().postDataJSON() as { targets: string[] };
      installs.push(body.targets);
      cursorConnected = true;
      await route.fulfill({
        json: {
          ok: true,
          reports: [{ id: 'cursor', display_name: 'Cursor', files: ['/home/me/.cursor/mcp.json'], notes: ['Restart Cursor.'] }],
        },
      });
    });

    await page.goto('/settings');
    const card = page.getByRole('region', { name: 'IDEs & agents' });
    const row = (id: string) => card.locator(`.ide-install-row[data-id="${id}"]`);
    await expect(row('claude')).toHaveAttribute('data-state', 'connected');
    await expect(row('claude')).toContainText('Connected');
    await expect(row('cursor')).toContainText('Found');
    await expect(row('zed')).toContainText('Not found');
    await expect(card).toContainText('1 found but not connected.');

    await card.getByRole('button', { name: 'Connect all found' }).click();
    await expect(card.getByRole('status')).toHaveText('Connected Cursor: /home/me/.cursor/mcp.json. Restart Cursor.');
    expect(installs).toEqual([['cursor']]);
    await expect(row('cursor')).toHaveAttribute('data-state', 'connected');
    await expect(row('cursor').getByRole('button', { name: 'Disconnect Cursor' })).toBeVisible();
    await expect(card.getByRole('button', { name: 'Connect all found' })).toBeDisabled();
  });

  test('a refused install shows the reason', async ({ page }) => {
    await page.route('**/api/agent/status', (route: Route) =>
      route.fulfill({
        json: {
          ok: true,
          targets: [
            { id: 'cursor', display_name: 'Cursor', detected: true, cli_on_path: false, cli_installable: false, configured: false, config_paths: [] },
          ],
          config: { enabled_targets: [], terminal_mode: 'builtin', active_profile: {}, profiles: {} },
          all_targets: ['cursor'],
        },
      }),
    );
    await page.route('**/api/agent/install', (route: Route) =>
      route.fulfill({ status: 403, json: { ok: false, error: 'Read-only mode' } }),
    );
    await page.goto('/settings');
    const card = page.getByRole('region', { name: 'IDEs & agents' });
    await card.getByRole('button', { name: 'Connect Cursor' }).click();
    await expect(card.getByRole('alert')).toHaveText('Read-only mode');
  });

  test('shows loading until the status arrives, never all-connected', async ({ page }) => {
    let release: () => void = () => {};
    const gate = new Promise<void>((r) => (release = r));
    await page.route('**/api/agent/status', async (route: Route) => {
      await gate;
      await route.fulfill({
        json: {
          ok: true,
          targets: [
            { id: 'cursor', display_name: 'Cursor', detected: true, cli_on_path: false, cli_installable: false, configured: false, config_paths: [] },
          ],
          config: { enabled_targets: [], terminal_mode: 'builtin', active_profile: {}, profiles: {} },
          all_targets: ['cursor'],
        },
      });
    });
    await page.goto('/settings');
    const card = page.getByRole('region', { name: 'IDEs & agents' });
    await expect(card).toContainText('Loading IDEs…');
    await expect(card).not.toContainText('Every IDE found here is connected.');
    await expect(card.getByRole('button', { name: 'Connect all found' })).toBeDisabled();
    release();
    await expect(card).toContainText('1 found but not connected.');
    await expect(card.locator('.ide-install-row[data-id="cursor"]')).toContainText('Found');
  });

  test('shows whether the Command Center panel is in each IDE', async ({ page }) => {
    const t = (id: string, display_name: string, panel: boolean | null) => ({
      id, display_name, detected: true, cli_on_path: false, cli_installable: false, configured: true, config_paths: [], panel,
    });
    await page.route('**/api/agent/status', (route: Route) =>
      route.fulfill({
        json: {
          ok: true,
          targets: [t('cursor', 'Cursor', true), t('jetbrains', 'JetBrains IDEs', false), t('claude', 'Claude Code', null)],
          config: { enabled_targets: [], terminal_mode: 'builtin', active_profile: {}, profiles: {} },
          all_targets: ['cursor', 'jetbrains', 'claude'],
        },
      }),
    );
    await page.goto('/settings');
    const card = page.getByRole('region', { name: 'IDEs & agents' });
    const row = (id: string) => card.locator(`.ide-install-row[data-id="${id}"]`);
    await expect(row('cursor')).toContainText('Panel');
    await expect(row('cursor')).not.toContainText('No panel');
    await expect(row('jetbrains')).toContainText('No panel');
    await expect(row('claude')).not.toContainText('anel');
  });

  test('a failed status load shows the error, not all-connected', async ({ page }) => {
    await page.route('**/api/agent/status', (route: Route) =>
      route.fulfill({ status: 500, json: { ok: false, error: 'boom' } }),
    );
    await page.goto('/settings');
    const card = page.getByRole('region', { name: 'IDEs & agents' });
    await expect(card).toContainText('Could not load IDEs.');
    await expect(card).not.toContainText('Every IDE found here is connected.');
  });
});
