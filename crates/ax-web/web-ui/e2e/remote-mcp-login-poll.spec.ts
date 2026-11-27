import { expect, test, type Page } from '@playwright/test';

type Poll = 'abort' | 'html502' | 'pending' | 'connected' | 'gone';

const connection = { url: 'https://ax.example.com/projects/demo/mcp', client_id: 'native', issuer: 'https://login.example', state: 'stored', expires_at: 0 };

async function serve(page: Page, polls: Poll[], seen: { status: number; list: number; maxInFlight: number }, slowMs = 0): Promise<void> {
  let connected = false;
  let inFlight = 0;
  await page.route('**/api/**', async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    if (path === '/api/remote-mcp/') {
      seen.list++;
      return route.fulfill({ json: { csrf: 'csrf', connections: connected ? [connection] : [] } });
    }
    if (path === '/api/remote-mcp/login' && request.method() === 'POST') {
      return route.fulfill({ json: { id: 'attempt-1', authorization_url: 'about:blank#authorize' } });
    }
    if (path === '/api/remote-mcp/login/attempt-1') {
      seen.status++;
      inFlight++;
      seen.maxInFlight = Math.max(seen.maxInFlight, inFlight);
      if (slowMs) await new Promise((r) => setTimeout(r, slowMs));
      inFlight--;
      const next = polls.length > 1 ? polls.shift()! : polls[0];
      if (next === 'abort') return route.abort('connectionreset');
      if (next === 'html502') return route.fulfill({ status: 502, contentType: 'text/html', body: '<h1>Bad gateway</h1>' });
      if (next === 'gone') return route.fulfill({ status: 404, json: { error: 'Sign-in attempt is unavailable' } });
      if (next === 'connected') {
        connected = true;
        return route.fulfill({ json: { state: 'connected' } });
      }
      return route.fulfill({ json: { state: 'pending' } });
    }
    if (path === '/api/remote-mcp/publications/') return route.fulfill({ json: { workspaceId: 'w', projectLabel: 'Demo', csrf: 'csrf', publications: [] } });
    if (path === '/api/ship/config') {
      return route.fulfill({ json: { config: { ship: { target_branch: 'main', web_port: 7070 }, remote: { provider: 'github' }, quality_gate: { steps: [], tests: {} }, sonar: {}, ui: {} } } });
    }
    if (path === '/api/web-auth/status') return route.fulfill({ json: { enabled: false, passkeys: true } });
    if (path === '/api/share/status') return route.fulfill({ json: { readonly: false, sharing: false, port: 7070 } });
    if (path === '/api/ship/review-language') return route.fulfill({ json: { code: 'en', languages: [] } });
    if (path === '/api/workspace/current') {
      return route.fulfill({ json: { ok: true, workspace: { path: '/demo', label: 'Demo', initialized: true }, recent: [] } });
    }
    if (path === '/api/stats') {
      return route.fulfill({ json: { readonly: false, project_name: 'Demo', node_count: 0, edge_count: 0, file_count: 0, languages: [], db_size_bytes: 0 } });
    }
    return route.fulfill({ status: 404, json: { error: 'Fixture endpoint not provided' } }).catch(() => {});
  });
  await page.setViewportSize({ width: 1600, height: 900 });
}

async function startSignIn(page: Page) {
  await page.goto('/settings?section=remote');
  const section = page.locator('.remote-mcp-settings');
  await section.getByLabel('Project MCP URL').fill(connection.url);
  await section.getByLabel('OAuth client ID').fill('native');
  await section.getByRole('button', { name: 'Connect with browser' }).click();
  await expect(section.getByRole('button', { name: 'Cancel sign-in' })).toBeVisible();
  return section;
}

test.describe('Remote MCP browser sign-in polling', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop settings');

  test('a dropped poll and a non-JSON poll reply keep waiting for the sign-in', async ({ page }) => {
    const seen = { status: 0, list: 0, maxInFlight: 0 };
    await serve(page, ['abort', 'html502', 'pending', 'connected'], seen);
    const section = await startSignIn(page);
    await expect(section.getByRole('status')).toHaveText(/Connected/, { timeout: 10_000 });
    await expect(section.getByRole('alert')).toHaveCount(0);
    await expect(section.getByText(connection.url, { exact: true })).toBeVisible();
  });

  test('a sign-in the server no longer knows stops polling and reloads connections', async ({ page }) => {
    const seen = { status: 0, list: 0, maxInFlight: 0 };
    await serve(page, ['gone'], seen);
    const section = await startSignIn(page);
    const listsBefore = seen.list;
    await expect(section.getByRole('alert')).toHaveText(/unavailable/, { timeout: 5_000 });
    await expect(section.getByRole('button', { name: 'Cancel sign-in' })).toHaveCount(0);
    await expect(section.getByRole('button', { name: 'Connect with browser' })).toBeEnabled();
    await expect.poll(() => seen.list).toBeGreaterThan(listsBefore);
    const polls = seen.status;
    await page.waitForTimeout(2_000);
    expect(seen.status).toBe(polls);
  });

  test('slow poll replies do not overlap', async ({ page }) => {
    const seen = { status: 0, list: 0, maxInFlight: 0 };
    await serve(page, ['pending'], seen, 2_500);
    await startSignIn(page);
    await page.waitForTimeout(6_000);
    expect(seen.status).toBeGreaterThanOrEqual(2);
    expect(seen.maxInFlight).toBe(1);
  });
});
