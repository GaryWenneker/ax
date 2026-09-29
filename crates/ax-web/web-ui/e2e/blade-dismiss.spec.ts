import { expect, test, type Page } from '@playwright/test';

const BLADE = '.memory-blade, .detail-panel--blade, .policy-inline-workspace';

const SELECTED_ROW = '.page-item--selected:not(.mcp-blade-step)';

const PAGES: { route: string; row: string }[] = [
  { route: '/memory', row: '.memory-vault-list .page-item--graph .page-item-title' },
  { route: '/logging', row: '.mcp-trace-row .page-item-title' },
  { route: '/policy/rules', row: '.policy-calm-row .page-item-title' },
  { route: '/policy/skills', row: '.policy-calm-row .page-item-title' },
  { route: '/nodes', row: '.page-item .page-item-title' },
  { route: '/unresolved', row: '.page-item .page-item-title' },
];

async function open(page: Page, route: string, row: string, i = 1) {
  await page.goto(route);
  const rows = page.locator(row);
  await rows.nth(i).click();
  await expect(page.locator(BLADE)).toHaveCount(1);
  return rows;
}

async function clickOutside(page: Page) {
  await page.mouse.click(700, 12);
}

for (const { route, row } of PAGES) {
  test.describe(`blade dismiss on ${route}`, () => {
    test('a click outside the blade and the list closes it', async ({ page }) => {
      await open(page, route, row);
      await clickOutside(page);
      await expect(page.locator(BLADE)).toHaveCount(0);
    });

    test('a click inside the blade keeps it open', async ({ page }) => {
      await open(page, route, row);
      await page.locator(BLADE).first().locator('.detail-header, .memory-blade-title, h2, h3').first().click();
      await page.waitForTimeout(400);
      await expect(page.locator(BLADE)).toHaveCount(1);
    });

    test('clicking the open row again closes it, and again reopens it', async ({ page }) => {
      const rows = await open(page, route, row);
      await expect(page.locator(SELECTED_ROW)).toHaveCount(1);
      await rows.nth(1).click();
      await expect(page.locator(BLADE)).toHaveCount(0);
      await expect(page.locator(SELECTED_ROW)).toHaveCount(0);
      await rows.nth(1).click();
      await expect(page.locator(BLADE)).toHaveCount(1);
      await expect(page.locator(SELECTED_ROW)).toHaveCount(1);
    });

    test('clicking another row switches the blade instead of closing it', async ({ page }) => {
      const rows = await open(page, route, row);
      await rows.nth(2).click();
      await page.waitForTimeout(400);
      await expect(page.locator(BLADE)).toHaveCount(1);
    });
  });
}
