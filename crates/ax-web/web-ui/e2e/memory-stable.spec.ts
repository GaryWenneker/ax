import { expect, test, type Page } from '@playwright/test';

const LISTS: { route: string; row: string }[] = [
  { route: '/memory', row: '.memory-vault-list .page-item--graph' },
  { route: '/policy/rules', row: '.policy-calm-row' },
  { route: '/policy/skills', row: '.policy-calm-row' },
  { route: '/logging', row: '.mcp-trace-row' },
];

const top = (page: Page, row: string) =>
  page.locator(row).first().evaluate((el) => Math.round(el.getBoundingClientRect().top));

for (const { route, row } of LISTS) {
  test(`${route}: opening and closing a blade does not move the list`, async ({ page }) => {
    await page.goto(route);
    await page.locator(row).nth(1).waitFor();
    await page.waitForTimeout(300);
    const before = await top(page, row);
    await page.locator(row).nth(1).locator('.page-item-title').click();
    await expect(page.locator('.memory-blade, .policy-inline-workspace')).toHaveCount(1);
    await page.waitForTimeout(300);
    const open = await top(page, row);
    await page.locator(row).nth(1).locator('.page-item-title').click();
    await expect(page.locator('.memory-blade, .policy-inline-workspace')).toHaveCount(0);
    await page.waitForTimeout(300);
    const closed = await top(page, row);
    expect(Math.abs(open - before)).toBeLessThanOrEqual(1);
    expect(Math.abs(closed - before)).toBeLessThanOrEqual(1);
  });
}

const TOGGLES: { route: string; row: string }[] = [
  { route: '/memory', row: '.memory-vault-list .page-item--graph' },
  { route: '/policy/rules', row: '.policy-calm-row' },
  { route: '/policy/skills', row: '.policy-calm-row' },
];

for (const { route, row } of TOGGLES) {
  test(`${route}: a deselected row's toggle looks like an untouched row's`, async ({ page }) => {
    await page.goto(route);
    const rows = page.locator(row).filter({ has: page.locator('.settings-toggle.on') });
    await rows.nth(3).waitFor();
    const clicked = rows.nth(1);
    await clicked.locator('.page-item-title').click();
    await clicked.locator('.page-item-title').click();
    await page.mouse.move(2, 400);
    await page.waitForTimeout(400);
    const opacity = (i: number) =>
      rows.nth(i).locator('.settings-toggle').evaluate((el) => getComputedStyle(el).opacity);
    expect(await opacity(1)).toBe(await opacity(3));
  });
}

test('memory rows color their node by kind category, with a legend', async ({ page }) => {
  await page.goto('/memory');
  const legend = page.locator('.memory-kind-legend');
  await expect(legend).toBeVisible();
  await expect(legend.locator('.memory-kind-legend-item--commit')).toHaveText('Commit');
  await expect(legend.locator('.memory-kind-legend-item--turn')).toHaveText('Chat turn');
  const border = (sel: string) =>
    page.locator(sel).first().locator('.page-item-node').evaluate((el) => getComputedStyle(el).borderTopColor);
  expect(await border('.memory-vault-list .memory-cat--commit')).toBe('rgb(240, 136, 62)');
  expect(await border('.memory-vault-list .memory-cat--turn')).toBe('rgb(79, 168, 255)');
  const legendItems = await legend.locator('[class*="memory-kind-legend-item--"]').count();
  const cats = await page.locator('.memory-vault-list [class*="memory-cat--"]').evaluateAll((els) =>
    new Set(els.map((e) => [...e.classList].find((c) => c.startsWith('memory-cat--')))).size,
  );
  expect(legendItems).toBe(cats);
});
