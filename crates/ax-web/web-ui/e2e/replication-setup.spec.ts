import { expect, test, type Page } from '@playwright/test';

async function fixture(page: Page, options: { readonly?: boolean; failPairing?: boolean } = {}) {
  const calls: { path: string; body: any }[] = [];
  let settings: any = null;
  let running = false;
  await page.route('**/api/**', async route => {
    const req = route.request(); const path = new URL(req.url()).pathname;
    let data: any;
    if (path === '/api/share/status') data = { readonly: options.readonly ?? false };
    else if (path === '/api/access/session') data = { csrf: 'csrf-test' };
    else if (path === '/api/access/nodes') data = { nodes: [], local: settings ? { node_id: 'stable', role: settings.config.role, sync_state: running ? 'running' : 'stopped' } : null };
    else if (path === '/api/access/nodes/local' && req.method() === 'GET') data = { settings, running, active_project: running };
    else if (path.startsWith('/api/access/nodes/local') && req.method() === 'POST') {
      expect(req.headers()['x-ax-access-csrf']).toBe('csrf-test');
      const body = req.postData() ? req.postDataJSON() : null; calls.push({ path, body });
      if (path.endsWith('/discover')) data = { candidates: [
        { node_id: 'office-primary', cluster_id: 'office', role: 'primary', advertised_url: 'https://office.example.com' },
        { node_id: 'secondary-primary', cluster_id: 'office', role: 'primary', advertised_url: 'https://secondary.example.com' },
        { node_id: 'replica', cluster_id: 'office', role: 'replica', advertised_url: 'https://replica.example.com' },
      ] };
      else if (path.endsWith('/stop')) { running = false; data = { stopped: true }; }
      else if (options.failPairing) { await route.fulfill({ status: 400, json: { error: 'Primary connection failed. Previous settings were kept.' } }); return; }
      else { running = true; settings = { config: { ...body, node_id: 'stable', projects: [{ id: body.project_id }] }, port: body.port, advertise: body.advertise }; data = { started: true, credential: body.role === 'primary' && !body.token ? { token: 'documentation-only-replication-credential' } : null }; }
    } else if (path === '/api/stats') data = { readonly: false, project_name: 'Fixture', node_count:0, edge_count:0, file_count:0, db_size_bytes:0, languages: [] };
    else if (path === '/api/ship/config') data = { config: { ship: {target_branch:'main',web_port:7070}, remote: {provider:'github'}, quality_gate: { steps: [], tests:{runner:'cargo test'} }, sonar: {}, ui: {} } };
    await route.fulfill({ status: data === undefined ? 404 : 200, json: data ?? {error:'Fixture endpoint not provided'} });
  });
  if (process.env.AX_ACCESS_SCREENSHOTS) await page.addInitScript(() => localStorage.setItem('ax-theme','azure-dark'));
  await page.goto('/settings?section=nodes');
  await expect(page.getByLabel('Local node role')).toBeVisible();
  return { calls };
}
async function ids(page: Page) { await page.getByLabel('Cluster ID', { exact: true }).fill('office'); await page.getByLabel('Replication project ID').fill('knowledge'); }

test('a local Primary can be started, stopped and restarted with preserved settings', async ({ page }) => {
  const { calls } = await fixture(page); await ids(page);
  await page.getByRole('button', { name: 'Start Primary', exact: true }).click();
  await expect(page.getByText('New replication credential — copy it now', { exact: true })).toBeVisible();
  await expect(page.getByLabel('Local node role')).toBeDisabled();
  expect(calls[0].body).toMatchObject({ role: 'primary', primary_url: null, cluster_id: 'office', project_id: 'knowledge', port: 9070 });
  await page.getByRole('button', { name: 'Stop managed node' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Cancel' }).click();
  expect(calls.some(c => c.path.endsWith('/stop'))).toBe(false);
  await page.getByRole('button', { name: 'Stop managed node' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Stop replication', exact: true }).click();
  await expect(page.getByLabel('Local node role')).toBeEnabled();
  await expect(page.getByLabel('Cluster ID', { exact: true })).toHaveValue('office');
  await expect(page.getByText('documentation-only-replication-credential', { exact: true })).toHaveCount(0);
});
test('discovery lists distinct primaries and only an explicit selection and connect sends pairing', async ({ page }) => {
  const { calls } = await fixture(page); await ids(page);
  await page.getByLabel('Local node role').selectOption('replica');
  await page.getByRole('button', { name: 'Find nearby Primaries' }).click();
  const choices = page.getByRole('group', {name:'Available Primaries'});
  await expect(choices.getByRole('button')).toHaveCount(2);
  await expect(page.getByLabel('Primary URL', { exact: true })).toHaveValue('');
  expect(calls.some(c => c.path === '/api/access/nodes/local')).toBe(false);
  await choices.getByRole('button', {name:/secondary-primary/}).click();
  await expect(choices.getByRole('button', {name:/secondary-primary/})).toHaveAttribute('aria-pressed','true');
  if (process.env.AX_ACCESS_SCREENSHOTS) { await expect(page.locator('#ax-green-vessel')).toHaveCount(0); await page.setViewportSize({width:1440,height:1600}); await page.getByRole('heading',{name:'Configure this Ax environment'}).scrollIntoViewIfNeeded(); await page.screenshot({path:'../../../site/public/screenshots/cc-primary-pairing.png',fullPage:true}); }
  await page.getByLabel('Replication credential', { exact: true }).fill('a'.repeat(64));
  await page.getByRole('button', { name: 'Connect to Primary', exact: true }).click();
  await expect(page.getByText('Replica is connected. Verified snapshots are staged for review.', { exact: true })).toBeVisible();
  expect(calls.find(c => c.path === '/api/access/nodes/local')?.body.primary_url).toBe('https://secondary.example.com');
});
test('manual hosted pairing failures remain editable and do not claim a connection', async ({ page }) => {
  await fixture(page, { failPairing: true }); await ids(page); await page.getByLabel('Local node role').selectOption('replica');
  await page.getByLabel('Primary URL', { exact: true }).fill('https://hosted.example.com');
  await page.getByLabel('Replication credential', { exact: true }).fill('b'.repeat(64));
  await page.getByRole('button', { name: 'Connect to Primary', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('Previous settings were kept');
  await expect(page.getByRole('button', { name: 'Connect to Primary', exact: true })).toBeEnabled();
  await expect(page.getByText('Replica is connected.', { exact: false })).toHaveCount(0);
});
test('read-only sessions cannot configure or discover nodes; setup fits a phone', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const { calls } = await fixture(page, { readonly: true });
  await expect(page.getByRole('button', { name: 'Start Primary', exact: true })).toBeDisabled();
  expect(calls).toEqual([]);
  const fits = await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth);
  expect(fits).toBe(true);
});
