import { expect, test, type Page } from '@playwright/test';

import { mockApi, policyRow, settledGraphNodes } from './fixtures';

const overlay = (page: Page) => page.getByRole('dialog', { name: 'Policy graph' });

const graphNode = (kind: 'rule' | 'skill' | 'memory', id: string, origin = 'project') => ({
  key: `${kind}:${origin}:${id}`,
  kind,
  id,
  label: id,
  origin,
  target: id,
  tags: ['misc'],
});

const policyDoc = (id: string) => ({
  frontmatter: policyRow({ id, name: id }),
  body: `Saved body of ${id}.`,
});

async function servePolicy(page: Page): Promise<void> {
  const rules = ['english-only', 'calm-lists'].map((id) => policyRow({ id, name: id, sourcePath: `.agents/rules/${id}.mdc` }));
  const skills = [policyRow({ id: 'pr', name: 'pr', sourcePath: '.agents/skills/pr/SKILL.md' })];
  await mockApi(page, {
    '/api/links/graph': {
      nodes: [
        graphNode('rule', 'english-only'),
        graphNode('rule', 'calm-lists'),
        graphNode('skill', 'pr'),
        graphNode('rule', 'house-style', 'global'),
      ],
      edges: [
        { source: 'rule:project:english-only', target: 'skill:project:pr' },
        { source: 'rule:project:calm-lists', target: 'skill:project:pr' },
      ],
    },
    '/api/links': { outgoing: [], backlinks: [] },
    '/api/policy/settings': { storage: 'files' },
    '/api/policy/rules/catalog/audits': [],
    '/api/policy/skills/catalog/audits': [],
    '/api/policy/rules': { rules },
    '/api/policy/skills': { skills },
    '/api/policy/rules/english-only': policyDoc('english-only'),
    '/api/policy/rules/calm-lists': policyDoc('calm-lists'),
    '/api/policy/skills/pr': policyDoc('pr'),
  });
}

async function openGraph(page: Page, path: string) {
  await servePolicy(page);
  await page.goto(path);
  await page.evaluate(() => localStorage.removeItem('ax.graph.policy.settings'));
  await page.reload();
  await page.locator('#main-content').getByRole('button', { name: 'Graph', exact: true }).click();
  await expect(overlay(page).locator('canvas.graph-canvas')).toBeVisible();
}

/** A project rule or skill node the pointer can reach. */
async function findNode(page: Page): Promise<{ key: string; x: number; y: number }> {
  const found = await settledGraphNodes(page, overlay(page));
  const node = found.find((n) => n.key.includes(':project:'));
  if (!node) throw new Error('no project node on the canvas');
  return node;
}

test.describe('Policy graph overlay', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop overlay');

  test('rules page opens the graph with legend and closed settings; Esc closes', async ({ page }) => {
    await openGraph(page, '/policy/rules');
    const o = overlay(page);
    for (const label of ['Rule', 'Skill', 'Memory', 'Global']) {
      await expect(o.locator('.graph-legend-label', { hasText: label })).toBeVisible();
    }
    const settings = page.getByRole('dialog', { name: 'Graph settings' });
    await expect(settings).toHaveCount(0);
    await o.getByRole('button', { name: 'Graph settings' }).click();
    await expect(settings).toBeVisible();
    const box = (await o.locator('canvas.graph-canvas').boundingBox())!;
    await page.mouse.click(box.x + 20, box.y + box.height - 20);
    await expect(settings).toHaveCount(0);
    await page.keyboard.press('Escape');
    await expect(o).toHaveCount(0);
  });

  test('skills page has the same Graph button', async ({ page }) => {
    await openGraph(page, '/policy/skills');
    await expect(overlay(page)).toBeVisible();
  });

  test('the open rule is preselected with its blade', async ({ page }) => {
    await openGraph(page, '/policy/rules?id=english-only');
    const o = overlay(page);
    await expect(o.locator('.graph-canvas-wrap')).toHaveAttribute('data-selected', 'rule:project:english-only');
    const blade = o.getByRole('complementary', { name: 'Edit rule' });
    await expect(blade).toBeVisible();
    await expect(blade.getByRole('button', { name: /^Save/ })).toBeVisible();
    await expect(o.getByRole('button', { name: 'Show selection' })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(blade).toHaveCount(0);
    await expect(o).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(o).toHaveCount(0);
  });

  test('clicking a rule or skill node opens its edit blade', async ({ page }) => {
    await openGraph(page, '/policy/rules');
    const o = overlay(page);
    const at = await findNode(page);
    const key = at.key;
    await page.mouse.move(at.x, at.y);
    await expect(o.locator('.graph-canvas-wrap')).toHaveAttribute('data-hover', key);
    await page.mouse.click(at.x, at.y);
    await expect(o.locator('.graph-canvas-wrap')).toHaveAttribute('data-selected', key);
    const kind = key.split(':')[0];
    const blade = o.getByRole('complementary', { name: kind === 'memory' ? 'Policy item' : `Edit ${kind}` });
    await expect(blade).toBeVisible();
    if (kind !== 'memory') await expect(blade.getByRole('button', { name: /^Save/ })).toBeVisible();
  });
});

test.describe('Auto-group', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop modal');

  for (const [path, noun] of [['/policy/rules', 'rules'], ['/policy/skills', 'skills']] as const) {
    test(`${noun} page previews suggestions without saving`, async ({ page }) => {
      await servePolicy(page);
      await page.goto(path);
      await page.locator('#main-content').getByRole('button', { name: 'Auto-group', exact: true }).click();
      const dialog = page.getByRole('dialog', { name: `Auto-group ${noun}` });
      await expect(dialog).toBeVisible();
      await expect(dialog.getByRole('button', { name: /^Apply \d+/ })).toBeVisible();
      await dialog.getByRole('button', { name: 'Cancel' }).click();
      await expect(dialog).toHaveCount(0);
    });
  }
});
