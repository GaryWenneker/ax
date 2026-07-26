import { expect, test } from '@playwright/test';

test.describe('Nodes list must scroll on mobile', () => {
  test('symbol browser list scrolls inside the card', async ({ page }) => {
    await page.setViewportSize({ width: 393, height: 851 });
    const stats = await page.request.get('/api/stats');
    expect(stats.ok()).toBeTruthy();

    await page.goto('/nodes');
    await expect(page.getByRole('heading', { name: 'Nodes' })).toBeVisible({ timeout: 15_000 });

    const list = page.locator('.page-split-main');
    await expect(list).toBeVisible({ timeout: 15_000 });
    await expect(page.locator('.page-item').first()).toBeVisible({ timeout: 15_000 });

    const metrics = await list.evaluate((el) => {
      const card = el.closest('.settings-card') as HTMLElement | null;
      const style = getComputedStyle(el);
      return {
        clientHeight: el.clientHeight,
        scrollHeight: el.scrollHeight,
        overflowY: style.overflowY,
        cardBottom: card?.getBoundingClientRect().bottom ?? 0,
        viewport: window.innerHeight,
        statusbarH: parseFloat(
          getComputedStyle(document.documentElement).getPropertyValue('--statusbar-h'),
        ),
      };
    });

    // List pane must be a real scrollport (content taller than visible area on a full graph)
    expect(metrics.overflowY === 'auto' || metrics.overflowY === 'scroll').toBeTruthy();
    expect(metrics.clientHeight).toBeGreaterThan(80);
    expect(metrics.scrollHeight).toBeGreaterThan(metrics.clientHeight + 20);

    // Card should fill the content column (padding above the fixed dock is OK)
    const dockTop = metrics.viewport - metrics.statusbarH;
    expect(dockTop - metrics.cardBottom).toBeLessThan(110);

    const before = await list.evaluate((el) => el.scrollTop);
    await list.evaluate((el) => {
      el.scrollTop = Math.min(el.scrollHeight, el.scrollTop + 240);
    });
    const after = await list.evaluate((el) => el.scrollTop);
    expect(after).toBeGreaterThan(before + 40);
  });
});
