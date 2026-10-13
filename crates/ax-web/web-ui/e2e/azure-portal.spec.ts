import { test, expect } from '@playwright/test';

const config = {
  ship: { target_branch: 'main', web_port: 7070, git_root: '/projects/ax', git_roots: ['/projects/ax'] },
  quality_gate: { steps: ['index', 'tia', 'tests', 'policy'], tests: { runner: 'cargo test' }, index_mode: 'incremental' },
  remote: { provider: 'github' }, ui: { show_savings: true, timezone: 'Europe/Amsterdam' },
};

// Deterministic design fixtures; these do not validate backend integrations.
const rules = ['typescript-safety', 'review-before-merge', 'document-public-api'].map((id, i) => ({
  id, level: ['CRITICAL', 'WARNING', 'INFO'][i], alwaysApply: true, globs: [], triggers: [],
  tags: ['quality'], priority: 100 - i * 10, body: `Keep ${id.replaceAll('-', ' ')} consistent.`,
  sourcePath: `.ax/rules/${id}.md`, enabled: true, scope: 'project', origin: 'project',
  effectiveStorage: 'files', group: 'Project standards',
}));
const memories = ['Azure design system', 'Preserve workspace preferences', 'Improve code navigation'].map((title, i) => ({
  id: `demo-${i}`, kind: ['decision', 'convention', 'note'][i], title,
  body: 'Design preview data for the Ax Command Center.', tags: ['design'], files: [],
  confidence: 0.95, source: 'agent', enabled: true, created_at: 1791676800000, updated_at: 1791676800000,
}));
const graphNodes = Array.from({ length: 24 }, (_, i) => ({ id: i + 1, name: ['App', 'Settings', 'Theme', 'Policy', 'Memory', 'Graph'][i % 6] + i,
  kind: 'function', file_path: `src/module${i % 4}.ts`, community_id: i % 4, community_label: `Module ${i % 4}`, degree: 3 }));
const fixtures: Record<string, unknown> = {
  '/api/web-auth/status': { enabled: false, passkeys: true },
  '/api/policy/settings': { policySync: false, requireReview: true, storage: 'files', roots: [] },
  '/api/policy/rules': { rules }, '/api/policy/skills': { skills: [] },
  '/api/memory': { memories, total: memories.length },
  '/api/stats': { node_count: 2450, edge_count: 7800, file_count: 312, languages: [{ language: 'Rust', count: 1500 }, { language: 'TypeScript', count: 950 }],
    last_indexed_at: 1791676800000, unresolved_ref_count: 0, db_size_bytes: 2400000, policy_rules_count: 3, policy_skills_count: 0, readonly: false, project_name: 'Ax · design preview' },
  '/api/graph': { nodes: graphNodes, edges: graphNodes.slice(1).map((n, i) => ({ source: i + 1, target: n.id, kind: 'calls', confidence: 'extracted' })), total_nodes: 24, truncated: false },
  '/api/insights': { nodeCount: 24, edgeCount: 23, numCommunities: 4, modularity: 0.7, godNodes: [], communities: [], surprisingConnections: [], suggestedQuestions: [] },
  '/api/domain-graph': { version: 1, nodes: [], edges: [] },
};

test.beforeEach(async ({ page }) => {
  await page.route('**/api/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    const data = path === '/api/ship/config' ? { config, sonar: {} }
      : path === '/api/ship/review-language' ? { ok: true, code: 'en', name: 'English', languages: [] }
      : path.includes('/search') ? { results: [], total: 0 } : fixtures[path] ?? null;
    await route.fulfill({ status: data ? 200 : 404, contentType: 'application/json', body: JSON.stringify(data ?? { error: 'Fixture endpoint not provided' }) });
  });
  await page.addInitScript(() => {
    if (!localStorage.getItem('ax-theme')) localStorage.setItem('ax-theme', 'azure-light');
    localStorage.setItem('ax-theme-mint-v2', '1');
  });
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/settings');
  await expect(page.getByRole('heading', { name: 'Appearance', exact: true })).toBeVisible();
});

test('all pages retain portal chrome in light and dark, including offline states', async ({ page }) => {
  test.setTimeout(120_000);
  await page.setViewportSize({ width: 1440, height: 1000 });
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  const paths = ['/stats', '/nodes', '/graph', '/files', '/search', '/memory', '/ship', '/sonar', '/agent', '/logging', '/savings', '/prices', '/unresolved', '/policy/rules', '/policy/skills', '/policy/rules/edit', '/policy/skills/edit', '/policy/match', '/policy/review', '/policy/sync'];
  for (const theme of ['azure-light', 'azure-dark']) {
    await page.evaluate((id) => localStorage.setItem('ax-theme', id), theme);
    for (const path of paths) {
      await page.goto(path);
      await expect(page.locator('html')).toHaveAttribute('data-ax-theme', theme);
      await expect(page.locator('.workspace')).toBeVisible();
      await expect(page.getByRole('navigation', { name: 'Main navigation' })).toBeVisible();
      if (process.env.AX_PORTAL_SCREENSHOTS && ['/stats', '/memory', '/policy/rules', '/graph'].includes(path)) {
        await page.waitForTimeout(500);
        await page.screenshot({ path: `/tmp/ax-portal-previews/${theme}-${path.replaceAll('/', '-')}.png` });
      }
    }
  }
  expect(errors).toEqual([]);
});

test('Light and Dark update chrome, survive reload, and remove legacy presets', async ({ page }) => {
  await expect(page.locator('html')).toHaveAttribute('data-ax-layout', 'portal');
  await expect(page.locator('html')).toHaveCSS('color-scheme', 'light');
  await page.getByRole('button', { name: 'Dark', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-ax-theme', 'azure-dark');
  await expect(page.locator('html')).toHaveCSS('color-scheme', 'dark');
  await expect(page.locator('.statusbar')).toHaveCSS('background-color', 'rgb(32, 32, 32)');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-ax-theme', 'azure-dark');
  await expect(page.getByRole('button', { name: 'ax Mint', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'macOS', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Light', exact: true }).click();
  await expect(page.locator('html')).toHaveCSS('color-scheme', 'light');
});

test('settings filter works and draft pipeline edits survive section changes', async ({ page }) => {
  const nav = page.getByRole('navigation', { name: 'Settings navigation' });
  await nav.getByRole('button', { name: 'Command Center', exact: true }).click();
  const draft = page.locator('#settings-panel-command-center input').first();
  await draft.fill('develop');
  await nav.getByRole('button', { name: 'Appearance', exact: true }).click();
  await expect(page.locator('#settings-panel-command-center')).toBeHidden();
  await nav.getByRole('button', { name: 'Command Center', exact: true }).click();
  await expect(draft).toHaveValue('develop');
  await page.getByRole('textbox', { name: 'Filter settings' }).fill('vault');
  await expect(nav.getByRole('button')).toHaveCount(1);
  await nav.getByRole('button', { name: 'Vault connection', exact: true }).click();
  await expect(page.locator('#settings-panel-vault')).toBeVisible();
  await page.getByRole('textbox', { name: 'Filter settings' }).fill('missing-section');
  await expect(page.getByText('No matching settings')).toBeVisible();
});

test('portal search navigates and replaces an existing code query', async ({ page }) => {
  const search = page.getByRole('combobox', { name: 'Search code or open a page' });
  const code = page.getByPlaceholder('Search symbols, functions, classes…');
  await search.fill('firstSymbol'); await search.press('Enter');
  await expect(page).toHaveURL(/\/search/); await expect(code).toHaveValue('firstSymbol');
  await search.fill('secondSymbol'); await search.press('Enter');
  await expect(code).toHaveValue('secondSymbol');
  await search.fill('Settings'); await search.press('Enter');
  await expect(page).toHaveURL(/\/settings/);
});

test('settings fits phone width and primary menu stays usable', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  if (process.env.AX_PORTAL_SCREENSHOTS) await page.screenshot({ path: '/tmp/ax-portal-previews/azure-light-phone.png' });
  await page.getByRole('button', { name: 'Toggle menu', exact: true }).click();
  await expect(page.getByRole('navigation', { name: 'Main navigation' })).toBeInViewport();
  await page.keyboard.press('Escape'); await expect(page.locator('.sidebar')).not.toHaveClass(/open/);
});

test('IDE embed keeps docked navigation and omits portal header', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 }); await page.goto('/settings?embed=1');
  await expect(page.locator('.titlebar')).toHaveCount(0);
  await expect(page.getByRole('navigation', { name: 'Main navigation' })).toBeInViewport();
  await expect(page.locator('.workspace')).toBeInViewport();
});
