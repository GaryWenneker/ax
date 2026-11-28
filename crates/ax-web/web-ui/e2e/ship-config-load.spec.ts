import { expect, test, type Page } from '@playwright/test';

import { emptyQualitySnapshot } from '../src/lib/mcpQuality';

const savedConfig = {
  ship: { target_branch: 'release', web_port: 7070 },
  quality_gate: { steps: ['tests'], tests: { runner: 'make check' }, index_mode: 'incremental' },
  remote: {
    provider: 'azure_devops',
    azure_devops: { org: 'acme', project: 'core', repo_id: 'r-1', token_env: 'AZDO_PAT' },
  },
  sonar: {
    enabled: false,
    host: 'http://127.0.0.1:9000',
    project_key: 'acme',
    token_env: 'SONAR_TOKEN',
    scanner_path: 'sonar-scanner',
    container_runtime: 'podman',
  },
  ui: { show_savings: true },
  reviewers: { lead: 'kim' },
};

type Serve = { configFails: boolean; puts: unknown[] };

async function serve(page: Page, configFails: boolean): Promise<Serve> {
  const state: Serve = { configFails, puts: [] };
  await page.route('**/api/**', (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    if (path === '/api/ship/config') {
      if (request.method() === 'PUT') {
        state.puts.push(request.postDataJSON());
        return route.fulfill({ json: { ok: true } });
      }
      if (state.configFails) return route.fulfill({ status: 503, json: { error: 'server restarting' } });
      return route.fulfill({
        json: { config: savedConfig, sonar: { runtimes: [], preferred: null, container: null, reachable: false, host: '' } },
      });
    }
    if (path === '/api/ship/review-language') {
      return route.fulfill({ json: { ok: true, code: 'en', name: 'English', languages: [] } });
    }
    if (path === '/api/stats') {
      return route.fulfill({ json: { project_name: 'Ship', node_count: 0, edge_count: 0, file_count: 0, languages: [], db_size_bytes: 0 } });
    }
    if (path === '/api/usage/mcp-quality') return route.fulfill({ json: { ...emptyQualitySnapshot(), projectLabel: 'Ship' } });
    if (path === '/api/web-auth/status') return route.fulfill({ json: { enabled: false } });
    if (path === '/api/vault/folders') return route.fulfill({ json: { folders: [] } });
    if (path.replace(/\/$/, '') === '/api/remote-mcp/publications') {
      return route.fulfill({ json: { workspaceId: 'fixture', projectLabel: 'Ship', csrf: 'csrf', publications: [] } });
    }
    if (path === '/api/usage/mcp-trace/events') return route.fulfill({ status: 404, json: { error: 'not mocked' } });
    return route.fulfill({ status: 404, json: { error: 'Fixture endpoint not provided' } });
  });
  return state;
}

test.describe('Ship config load', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop settings layout');

  test('Settings cannot save the built-in defaults after a failed load', async ({ page }) => {
    const state = await serve(page, true);
    await page.goto('/settings?section=appearance');
    await expect(page.getByText('server restarting').first()).toBeVisible();
    await expect(page.getByRole('switch', { name: 'Show Savings page' })).toBeDisabled();
    await expect(page.getByLabel('Timezone for Logging timestamps')).toBeDisabled();
    await page.goto('/settings?section=command-center');
    await expect(page.getByText('server restarting').first()).toBeVisible();
    await expect(page.getByRole('button', { name: 'Save settings' })).toBeDisabled();
    expect(state.puts).toEqual([]);
  });

  test('the Savings toggle saves only its own field', async ({ page }) => {
    const state = await serve(page, false);
    await page.goto('/settings?section=command-center');
    const branch = page.locator('#settings-panel-command-center input.settings-input').first();
    await expect(branch).toHaveValue('release');
    await branch.fill('half-typ');
    await page.getByRole('button', { name: 'Appearance', exact: true }).click();
    await page.getByRole('switch', { name: 'Show Savings page' }).click();
    await expect.poll(() => state.puts.length).toBe(1);
    const body = state.puts[0] as typeof savedConfig;
    expect(body.ui.show_savings).toBe(false);
    expect(body.ship.target_branch).toBe('release');
    expect(body.reviewers).toEqual({ lead: 'kim' });
  });
});
