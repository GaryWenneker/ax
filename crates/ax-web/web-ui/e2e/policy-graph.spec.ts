import { expect, test, type Page } from '@playwright/test';

const overlay = (page: Page) => page.getByRole('dialog', { name: 'Policy graph' });

async function openGraph(page: Page, path: string) {
  await page.goto(path);
  await page.evaluate(() => localStorage.removeItem('ax.graph.policy.settings'));
  await page.reload();
  await page.locator('#main-content').getByRole('button', { name: 'Graph', exact: true }).click();
  await expect(overlay(page).locator('canvas.graph-canvas')).toBeVisible();
}

async function findNode(page: Page): Promise<{ x: number; y: number }> {
  const wrap = overlay(page).locator('.graph-canvas-wrap');
  await page.waitForTimeout(4000);
  const box = (await overlay(page).locator('canvas.graph-canvas').boundingBox())!;
  for (let y = box.y + 60; y < box.y + box.height - 60; y += 6) {
    for (let x = box.x + 200; x < box.x + box.width - 200; x += 6) {
      await page.mouse.move(x, y);
      if (await wrap.getAttribute('data-hover')) return { x, y };
    }
  }
  throw new Error('no node found under the cursor');
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
    const key = (await o.locator('.graph-canvas-wrap').getAttribute('data-hover'))!;
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
