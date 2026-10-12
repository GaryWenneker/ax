import { expect, test, type Page, type Route } from '@playwright/test';

async function fixture(page: Page, options: { readonly?: boolean; failPairing?: boolean; conflicts?: boolean; syncFailed?: boolean; configuredRole?: 'primary' | 'replica'; stopped?: boolean } = {}) {
  const calls: { path: string; body: any }[] = [];
  let settings: any = options.configuredRole ? { config: { node_id: 'stable', role: options.configuredRole, cluster_id: 'office', primary_url: options.configuredRole === 'replica' ? 'https://office.example.com' : null, projects: [{ id: 'knowledge' }] }, port: 9070, advertise: null } : null;
  let running = !!options.configuredRole && !options.stopped;
  let storedToken = 'documentation-only-replication-credential';
  await page.route('**/api/**', async route => {
    const req = route.request(); const path = new URL(req.url()).pathname;
    let data: any;
    if (path === '/api/share/status') data = { readonly: options.readonly ?? false };
    else if (path === '/api/access/session') data = { csrf: 'csrf-test' };
    else if (path === '/api/access/nodes') data = { nodes: [], local: settings ? { node_id: 'stable', role: settings.config.role, sync_state: running ? 'running' : 'stopped' } : null };
    else if (path === '/api/access/nodes/local/interfaces') data = { interfaces: [{ name: 'Ethernet', address: '192.168.1.20', loopback: false }, { name: 'lo', address: '127.0.0.1', loopback: true }] };
    else if (path === '/api/access/nodes/local' && req.method() === 'GET') data = { settings, running, active_project: running, active_replicas: running ? 2 : 0, sync: running ? {status: 'full_data_synced', last_success_unix_seconds: 1791800000} : null, safety: {mode:"preserve_and_exchange",memories:412,rules:27,skills:8,review_required:options.conflicts,sync_failed:options.syncFailed} };
    else if (path.startsWith('/api/access/nodes/local') && req.method() === 'POST') {
      expect(req.headers()['x-ax-access-csrf']).toBe('csrf-test');
      const body = req.postData() ? req.postDataJSON() : null; calls.push({ path, body });
      if (path.endsWith('/discover')) data = { candidates: [
        { node_id: 'office-primary', cluster_id: 'office', role: 'primary', project_ids: ['knowledge'], advertised_url: 'https://office.example.com' },
        { node_id: 'secondary-primary', cluster_id: 'office', role: 'primary', project_ids: ['knowledge'], advertised_url: 'https://secondary.example.com' },
        { node_id: 'replica', cluster_id: 'office', role: 'replica', advertised_url: 'https://replica.example.com' },
      ] };
      else if (path.endsWith('/credential')) data = { token: storedToken };
      else if (path.endsWith('/remove')) { running = false; settings = null; data = { removed: true }; }
      else if (path.endsWith('/stop')) { running = false; data = { stopped: true }; }
      else if (options.failPairing) { await route.fulfill({ status: 400, json: { error: 'Primary connection failed. Previous settings were kept.' } }); return; }
      else { running = true; const generated = !settings && !body.token && body.role === 'primary'; storedToken = body.token || storedToken; settings = { config: { ...body, cluster_id: body.cluster_id || 'cluster_generated', node_id: 'stable', projects: [{ id: body.project_id || 'project_generated' }] }, port: body.port || 9877, advertise: body.advertise?.replace(':0', ':9877') }; data = { started: true, credential: generated ? { token: storedToken } : null }; }
    } else if (path === '/api/stats') data = { readonly: false, project_name: 'Fixture', node_count:0, edge_count:0, file_count:0, db_size_bytes:0, languages: [] };
    else if (path === '/api/ship/config') data = { config: { ship: {target_branch:'main',web_port:7070}, remote: {provider:'github'}, quality_gate: { steps: [], tests:{runner:'cargo test'} }, sonar: {}, ui: {} } };
    await route.fulfill({ status: data === undefined ? 404 : 200, json: data ?? {error:'Fixture endpoint not provided'} });
  });
  if (process.env.AX_ACCESS_SCREENSHOTS) await page.addInitScript(() => localStorage.setItem('ax-theme','azure-dark'));
  await page.goto('/settings?section=nodes');
  await expect(page.getByRole('heading', {name: options.configuredRole ? 'Replication configuration' : 'Configure this Ax environment', exact:true})).toBeVisible();
  return { calls };
}
async function ids(page: Page) {
  await page.getByRole('button', {name:'Set up replication',exact:true}).click();
  await page.getByText('Advanced options',{exact:true}).click(); await page.getByLabel('Cluster ID', { exact: true }).fill('office'); await page.getByLabel('Replication project ID').fill('knowledge'); }

test('empty Primary setup generates IDs, hides the form, masks credentials and can be removed', async ({ page, context }) => {
  const { calls } = await fixture(page);
  await page.getByRole('button', { name: 'Set up replication', exact: true }).click();
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.getByRole('button', { name: 'Start Primary', exact: true }).click();
  const saved = page.getByLabel('Saved replication configuration');
  await expect(saved).toBeVisible();
  await expect(page.getByRole('form', {name:'Replication setup'})).toHaveCount(0);
  await expect(saved).toContainText('cluster_generated');
  await expect(saved).toContainText('project_generated');
  expect(calls[0].body).toMatchObject({ role: 'primary', primary_url: null, cluster_id: '', project_id: '', port: 0, advertise: 'http://192.168.1.20:0' });
  const credential = page.getByLabel('Replication credential', {exact:true});
  await expect(credential).toHaveAttribute('type','password');
  await page.getByRole('button',{name:'Show replication credential',exact:true}).click();
  await expect(credential).toHaveValue('documentation-only-replication-credential');
  await expect(credential).toHaveAttribute('type','text');
  await page.getByRole('button',{name:'Copy replication credential',exact:true}).click();
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe('documentation-only-replication-credential');
  await page.getByRole('button',{name:'Hide replication credential',exact:true}).click();
  await expect(credential).toHaveAttribute('type','password');
  if (process.env.AX_ACCESS_SCREENSHOTS && test.info().project.name === 'desktop-chrome') {
    await page.setViewportSize({width:1440,height:1300});
    await page.screenshot({path:'../../../site/public/screenshots/cc-replication-configured.png',fullPage:true});
  }
  await page.getByRole('button', { name: 'Stop managed node' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Cancel' }).click();
  expect(calls.some(c => c.path.endsWith('/stop'))).toBe(false);
  await page.getByRole('button', { name: 'Remove local configuration' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Cancel' }).click();
  expect(calls.some(c => c.path.endsWith('/remove'))).toBe(false);
  await page.getByRole('button', { name: 'Remove local configuration' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Remove configuration',exact:true }).click();
  await expect(page.getByRole('button', { name: 'Set up replication', exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Local Primary', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Set up replication', exact: true }).click();
  await page.getByText('Advanced options', { exact: true }).click();
  await expect(page.getByLabel('Cluster ID',{exact:true})).toHaveValue('');
  await expect(page.getByLabel('Replication credential',{exact:true})).toHaveValue('');
});

test('discovery lists distinct primaries and only an explicit selection and connect sends pairing', async ({ page }) => {
  const { calls } = await fixture(page); await page.getByRole('button', { name: 'Set up replication', exact: true }).click();
  await page.getByLabel('Local node role').selectOption('replica');
  await expect(page.getByRole('button', {name:'Find nearby Primaries'})).toHaveCount(0);
  const choices = page.getByRole('group', {name:'Available Primaries'});
  await expect(choices.getByRole('button')).toHaveCount(2);
  await expect(page.getByLabel('Primary URL', { exact: true })).toHaveValue('');
  expect(calls.some(c => c.path === '/api/access/nodes/local')).toBe(false);
  await expect(page.getByRole('note',{name:'Existing data protection'})).toContainText('412');
  await expect(page.getByRole('note',{name:'Existing data protection'})).toContainText('An empty Primary cannot clear');
  await choices.getByRole('button', {name:/secondary-primary/}).click();
  await expect(choices.getByRole('button', {name:/secondary-primary/})).toHaveAttribute('aria-pressed','true');
  if (process.env.AX_ACCESS_SCREENSHOTS && test.info().project.name === 'desktop-chrome') { await expect(page.locator('#ax-green-vessel')).toHaveCount(0); await page.setViewportSize({width:1440,height:1800}); await page.getByRole('heading',{name:'Configure this Ax environment'}).scrollIntoViewIfNeeded(); await page.screenshot({path:'../../../site/public/screenshots/cc-primary-pairing.png',fullPage:true}); }
  await page.getByLabel('Replication credential', { exact: true }).fill('a'.repeat(64));
  await page.getByRole('button', { name: 'Connect to Primary', exact: true }).click();
  await expect(page.getByText('Replica connected. Graph, source, logs and memory are synchronized. Received policies require review.', { exact: true })).toBeVisible();
  expect(calls.find(c => c.path === '/api/access/nodes/local')?.body.primary_url).toBe('https://secondary.example.com');
});
test('manual hosted pairing failures remain editable and do not claim a connection', async ({ page }) => {
  await fixture(page, { failPairing: true }); await page.getByRole('button', { name: 'Set up replication', exact: true }).click(); await page.getByLabel('Local node role').selectOption('replica');
  await page.getByLabel('Primary URL', { exact: true }).fill('https://hosted.example.com');
  await page.getByLabel('Replication credential', { exact: true }).fill('b'.repeat(64));
  await page.getByRole('button', { name: 'Connect to Primary', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('Previous settings were kept');
  await expect(page.getByRole('button', { name: 'Connect to Primary', exact: true })).toBeEnabled();
  await expect(page.getByText('Replica connected.', { exact: false })).toHaveCount(0);
});
test('read-only sessions cannot configure or discover nodes; setup fits a phone', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const { calls } = await fixture(page, { readonly: true });
  await expect(page.getByRole('button', { name: 'Set up replication', exact: true })).toBeDisabled();
  expect(calls).toEqual([]);
  const fits = await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth);
  expect(fits).toBe(true);
});

 test('retained local edits and failed synchronization are visible without hiding existing counts', async ({ page }) => {
  await fixture(page, {conflicts:true,syncFailed:true});
  const safety=page.getByRole('note',{name:'Existing data protection'});
  await expect(safety).toContainText('412');
  await expect(safety).toContainText('Conflicting local edits were preserved');
  await expect(safety.getByRole('alert')).toContainText('Existing local data remains intact');
 });

test('a delayed pre-start status cannot unlock a running node', async ({ page }) => {
  await fixture(page);
  await ids(page);
  let delayedStatus: Route | undefined;
  await page.route('**/api/access/nodes/local', async route => {
    if (route.request().method() === 'GET' && !delayedStatus) {
      delayedStatus = route;
      return;
    }
    await route.fallback();
  });
  await expect.poll(() => Boolean(delayedStatus),{timeout:7000}).toBe(true);
  await page.getByRole('button', { name: 'Start Primary', exact: true }).click();
  await expect(page.getByLabel('Saved replication configuration')).toBeVisible();
  await delayedStatus!.fulfill({ json: { settings: null, running: false, active_project: false } });
  // Wait for the stale response to be delivered before checking the controls.
  await page.waitForTimeout(100);
  await expect(page.getByRole('button', { name: 'Stop managed node' })).toBeVisible();
  await expect(page.getByRole('form',{name:'Replication setup'})).toHaveCount(0);
});

for (const configuredRole of ['primary', 'replica'] as const) {
  test(`saved ${configuredRole} configuration stays hidden after reload and can restart`, async ({ page }) => {
    const { calls } = await fixture(page,{configuredRole,stopped:true});
    await expect(page.getByRole('form',{name:'Replication setup'})).toHaveCount(0);
    await page.reload();
    await expect(page.getByLabel('Saved replication configuration')).toBeVisible();
    await page.getByRole('button',{name: configuredRole === 'primary' ? 'Restart Primary' : 'Reconnect Replica',exact:true}).click();
    await page.getByRole('dialog').getByRole('button',{name: configuredRole === 'primary' ? 'Start Primary' : 'Connect to Primary',exact:true}).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByRole('button',{name:'Stop managed node'})).toBeVisible();
    expect(calls.find(c => c.path === '/api/access/nodes/local')?.body).toMatchObject({cluster_id:'office',project_id:'knowledge',token:null});
  });
}

test('a failed synchronization is not shown as a successful active connection', async ({ page }) => {
  await fixture(page,{configuredRole:'replica',syncFailed:true});
  await expect(page.getByLabel('Replication connection')).not.toHaveClass(/is-connected/);
  await expect(page.getByRole('note',{name:'Existing data protection'}).getByRole('alert')).toBeVisible();
});
