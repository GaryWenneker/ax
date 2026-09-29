import { expect, test } from '@playwright/test';

test.describe('Obsidian-style graph settings', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/graph');
    await page.evaluate(() => localStorage.removeItem('ax.graph.settings'));
    await page.reload();
  });

  const openSettings = async (page: import('@playwright/test').Page) => {
    await page.getByRole('button', { name: 'Graph settings' }).click();
    return page.getByRole('dialog', { name: 'Graph settings' });
  };

  test('settings start closed, open via the button, close on outside click', async ({ page }) => {
    const panel = page.getByRole('dialog', { name: 'Graph settings' });
    await expect(panel).toHaveCount(0);
    await openSettings(page);
    await expect(panel).toBeVisible();
    await panel.getByLabel('Node size').click();
    await expect(panel).toBeVisible();
    const box = (await page.locator('canvas.graph-canvas').boundingBox())!;
    await page.mouse.click(box.x + 20, box.y + box.height - 20);
    await expect(panel).toHaveCount(0);
  });

  test('panel shows Filters, Groups, Display, and Forces', async ({ page }) => {
    const panel = await openSettings(page);
    await expect(panel).toBeVisible();
    for (const title of ['Filters', 'Groups', 'Display', 'Forces']) {
      await expect(panel.getByRole('button', { name: title, exact: false }).first()).toBeVisible();
    }
    await expect(panel.getByLabel('Repel force')).toHaveValue('10');
    await expect(panel.getByLabel('Link distance')).toHaveValue('250');
  });

  test('hovering a node marks it as hovered', async ({ page }, info) => {
    test.skip(info.project.name === 'mobile-chrome', 'no mouse hover on touch');
    const wrap = page.locator('.graph-canvas-wrap');
    await page.waitForTimeout(6000);
    const box = (await page.locator('canvas.graph-canvas').boundingBox())!;
    let hovered = '';
    for (let y = box.y + 40; y < box.y + box.height - 40 && !hovered; y += 8) {
      for (let x = box.x + 250; x < box.x + box.width - 300 && !hovered; x += 8) {
        await page.mouse.move(x, y);
        hovered = (await wrap.getAttribute('data-hover')) ?? '';
      }
    }
    expect(hovered).not.toBe('');
  });

  test('clicking a node opens a detail blade', async ({ page }, info) => {
    test.skip(info.project.name === 'mobile-chrome', 'desktop hover scan');
    const wrap = page.locator('.graph-canvas-wrap');
    await page.waitForTimeout(6000);
    const box = (await page.locator('canvas.graph-canvas').boundingBox())!;
    let hovered = '';
    for (let y = box.y + 40; y < box.y + box.height - 40 && !hovered; y += 8) {
      for (let x = box.x + 250; x < box.x + box.width - 300 && !hovered; x += 8) {
        await page.mouse.move(x, y);
        hovered = (await wrap.getAttribute('data-hover')) ?? '';
      }
    }
    await page.mouse.down();
    await page.mouse.up();
    const blade = page.locator('aside.detail-panel');
    await expect(blade).toBeVisible();
    await expect(blade).toContainText(hovered);
    const canvasWidth = (await page.locator('canvas.graph-canvas').boundingBox())!.width;
    expect(canvasWidth).toBeGreaterThan(400);
    await page.getByRole('button', { name: /Show selection/ }).click();
    await expect(blade).toBeVisible();
  });

  test('slider changes persist and reset restores defaults', async ({ page }) => {
    const panel = await openSettings(page);
    await panel.getByLabel('Link distance').fill('400');
    await page.reload();
    await openSettings(page);
    await expect(page.getByRole('dialog', { name: 'Graph settings' }).getByLabel('Link distance')).toHaveValue('400');
    await page.getByTitle('Reset to defaults').click();
    await expect(page.getByRole('dialog', { name: 'Graph settings' }).getByLabel('Link distance')).toHaveValue('250');
  });
});
