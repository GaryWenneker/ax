import { expect, test, type Page } from '@playwright/test';

async function rightClickPrevented(page: Page, selector: string): Promise<boolean> {
  return page.locator(selector).first().evaluate((el) => {
    const ev = new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2 });
    el.dispatchEvent(ev);
    return ev.defaultPrevented;
  });
}

test.describe('App feel', () => {
  test('row hover actions form a tight icon group', async ({ page }) => {
    await page.goto('/policy/rules');
    const row = page.locator('.policy-calm-row').first();
    await row.hover();
    const boxes = await row.evaluate((r) =>
      [...r.querySelectorAll('.calm-row-action')].map((b) => {
        const box = b.getBoundingClientRect();
        return { left: box.left, right: box.right, width: box.width };
      }),
    );
    expect(boxes.length).toBeGreaterThanOrEqual(2);
    for (const b of boxes) expect(b.width).toBeLessThanOrEqual(22);
    for (let i = 1; i < boxes.length; i++) expect(boxes[i].left - boxes[i - 1].right).toBeLessThanOrEqual(2);
  });

  test('row controls line up in columns across rows', async ({ page }) => {
    for (const route of ['/policy/rules', '/policy/skills']) {
      await page.goto(route);
      await page.locator('.policy-calm-row').first().waitFor();
      const cols = await page.locator('.policy-calm-row').evaluateAll((rows) =>
        rows.map((r) => {
          const x = (sel: string) => Math.round((r.querySelector(sel) as Element).getBoundingClientRect().left);
          return { meta: x('.page-item-meta'), check: x('.calm-row-check'), last: x('.calm-row-aside > :last-child') };
        }),
      );
      expect(cols.length, route).toBeGreaterThan(1);
      for (const key of ['meta', 'check', 'last'] as const) {
        expect(new Set(cols.map((c) => c[key])).size, `${route} ${key}`).toBe(1);
      }
    }
  });

  test('toggles are dimmed until their row is hovered, and on stays visible', async ({ page }) => {
    await page.goto('/policy/rules');
    const row = page.locator('.policy-calm-row').filter({ has: page.locator('.settings-toggle.on') }).first();
    const on = row.locator('.settings-toggle.on');
    await page.mouse.move(0, 0);
    const dim = await on.evaluate((el) => Number(getComputedStyle(el).opacity));
    expect(dim).toBeLessThan(1);
    expect(dim).toBeGreaterThan(0.3);
    await row.locator('.page-item-title').hover();
    await expect.poll(() => on.evaluate((el) => Number(getComputedStyle(el).opacity))).toBe(1);
    const others = await page
      .locator('.policy-calm-row .settings-toggle')
      .evaluateAll((els, hovered) => els.filter((el) => el !== hovered && Number(getComputedStyle(el).opacity) === 1).length, await on.elementHandle());
    expect(others).toBe(0);
  });

  test('right-click does nothing outside text fields', async ({ page }) => {
    await page.goto('/stats');
    await expect(page.locator('main').first()).toBeVisible();
    expect(await rightClickPrevented(page, 'main')).toBe(true);
    await page.goto('/logging');
    await page.locator('.mcp-trace-row').first().waitFor();
    expect(await rightClickPrevented(page, '.mcp-trace-row')).toBe(true);
    expect(await rightClickPrevented(page, 'input[type="search"], input[placeholder]')).toBe(false);
  });

  test('own row menu still opens on right-click', async ({ page }) => {
    await page.goto('/policy/rules');
    await page.locator('.policy-calm-row').first().click({ button: 'right' });
    await expect(page.locator('[role="menu"]').first()).toBeVisible();
  });
});
