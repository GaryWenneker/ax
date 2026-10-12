import { test, expect } from '@playwright/test';

test('host setup generates configuration and client IDs, persists status and controls project approvals', async ({ page }) => {
  let running = false;
  let configured = false;
  let approval = 'pending';
  let setup: unknown;
  const workspaceId = 'host-workspace';
  await page.route('**/api/remote-mcp/publications**', async route => {
    const path = new URL(route.request().url()).pathname;
    let value: unknown;
    if (path.endsWith('/host/setup')) {
      expect(route.request().headers()['x-ax-csrf']).toBe('host-csrf');
      setup = route.request().postDataJSON() as unknown;
      configured = running = true;
      value = { workspaceId, updated: true };
    } else if (path.endsWith('/host/action')) {
      expect(route.request().headers()['x-ax-csrf']).toBe('host-csrf');
      const body = route.request().postDataJSON() as { operation: string };
      if (body.operation === 'remove') configured = running = false;
      else running = body.operation === 'start';
      value = { workspaceId, updated: true };
    } else if (path.endsWith('/authorizations/request-1')) {
      const body = route.request().postDataJSON() as { decision: string };
      approval = body.decision === 'approve' ? 'delivered' : 'revoked';
      value = { workspaceId, updated: true };
    } else if (path.endsWith('/host/authorizations')) value = { workspaceId, requests: configured ? [{ id: 'request-1', clientName: 'Erik’s IDE', scope: 'ax:context', status: approval === 'revoked' ? 'delivered' : approval, expires: 9999999999, revoked: approval === 'revoked', redirectUri: 'http://127.0.0.1:8999/callback' }] : [] };
    else if (path.endsWith('/host')) value = { workspaceId, host: configured ? { status: running ? 'running' : 'stopped', message: null, publicUrl: 'https://ax.example.com', localUrl: 'http://127.0.0.1:9876', builtInOAuth: true, autoStart: running } : null };
    else value = { workspaceId, projectLabel: 'Demo', csrf: 'host-csrf', publications: [] };
    await route.fulfill({ json: value });
  });
  await page.goto('/settings');
  await page.getByRole('navigation', { name: 'Settings navigation' }).getByRole('button', { name: 'Remote MCP', exact: true }).click();
  const host = page.getByRole('region', { name: 'Remote MCP host' });
  await expect(host).toContainText('Remote MCP not configured');
  await host.getByRole('button', { name: 'Set up remote MCP', exact: true }).click();
  const modal = page.getByRole('dialog', { name: 'Set up remote MCP host' });
  await modal.getByLabel('Public HTTPS origin').fill('https://ax.example.com');
  await modal.getByText('Advanced OAuth options', { exact: true }).click();
  await expect(modal).toContainText('Ax supplies its own issuer');
  await expect(modal.getByLabel('Native Ax public client ID', { exact: true })).toBeDisabled();
  await modal.getByRole('button', { name: 'Generate and start host' }).click();
  await expect(modal).toHaveCount(0);
  expect(setup).toMatchObject({ workspaceId, publicUrl: 'https://ax.example.com', config: null, clients: { axProxy: null, cursor: null, vscode: null } });
  await expect(host).toContainText('Remote MCP running');
  await page.reload();
  await page.getByRole('navigation', { name: 'Settings navigation' }).getByRole('button', { name: 'Remote MCP', exact: true }).click();
  await expect(host).toContainText('Remote MCP running');
  await host.getByRole('button', { name: 'Approve context access' }).click();
  await expect(host).toContainText('delivered');
  await host.getByRole('button', { name: 'Revoke access' }).click();
  await expect(host).toContainText('Revoked');
  await host.getByRole('button', { name: 'Stop host', exact: true }).click();
  await expect(host).toContainText('Remote MCP stopped');
  await host.getByRole('button', { name: 'Start host', exact: true }).click();
  await expect(host).toContainText('Remote MCP running');
  if (process.env.AX_ACCESS_SCREENSHOTS && test.info().project.name === 'desktop-chrome') {
    await page.screenshot({ path: '../../../site/public/screenshots/cc-remote-host.png', fullPage: true });
  }
  await host.getByRole('button', { name: 'Remove host', exact: true }).click();
  await page.getByRole('dialog', { name: 'Remove remote MCP host?' }).getByRole('button', { name: 'Remove host', exact: true }).click();
  await expect(host).toContainText('Remote MCP not configured');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});
