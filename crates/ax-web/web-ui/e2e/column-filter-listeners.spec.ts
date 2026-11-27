import { expect, test, type Page } from '@playwright/test';

import { mockApi, policyRow } from './fixtures';

/** Counts live document `focusin` listeners and column-width storage writes. */
async function instrument(page: Page) {
  await page.addInitScript(() => {
    const w = window as unknown as { __focusin: number; __widthWrites: string[] };
    w.__focusin = 0;
    w.__widthWrites = [];
    const add = Document.prototype.addEventListener;
    const remove = Document.prototype.removeEventListener;
    Document.prototype.addEventListener = function (this: Document, type: string, ...rest: unknown[]) {
      if (type === 'focusin') w.__focusin += 1;
      return (add as (...a: unknown[]) => void).call(this, type, ...rest);
    } as typeof Document.prototype.addEventListener;
    Document.prototype.removeEventListener = function (this: Document, type: string, ...rest: unknown[]) {
      if (type === 'focusin') w.__focusin -= 1;
      return (remove as (...a: unknown[]) => void).call(this, type, ...rest);
    } as typeof Document.prototype.removeEventListener;
    const setItem = Storage.prototype.setItem;
    Storage.prototype.setItem = function (this: Storage, key: string, value: string) {
      if (key.startsWith('ax-list-columns')) w.__widthWrites.push(value);
      return setItem.call(this, key, value);
    };
  });
}

async function openRules(page: Page) {
  await instrument(page);
  await mockApi(page, {
    '/api/policy/rules': { rules: [policyRow()] },
    '/api/policy/settings': { storage: 'files' },
    '/api/policy/rules/catalog/audits': [],
  });
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto('/policy/rules');
  await expect(page.getByRole('button', { name: 'Sort Name' })).toBeVisible();
}

const focusinListeners = (page: Page) => page.evaluate(() => (window as unknown as { __focusin: number }).__focusin);

test('closed column filters keep no document listeners; an open one adds them until it closes', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  await openRules(page);
  const filters = page.locator('details.column-filter');
  expect(await filters.count()).toBeGreaterThan(1);
  const idle = await focusinListeners(page);

  await filters.first().locator('summary').click();
  await expect(page.locator('.column-filter-panel:popover-open')).toHaveCount(1);
  expect(await focusinListeners(page)).toBe(idle + 1);

  await page.mouse.click(5, 5);
  await expect(page.locator('.column-filter-panel:popover-open')).toHaveCount(0);
  expect(await focusinListeners(page)).toBe(idle);

  // The idle count does not grow with the number of filterable columns.
  expect(idle).toBeLessThan(await filters.count());
});

test('dragging a column border saves the width once, on release', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  await openRules(page);
  const handle = page.getByRole('separator', { name: 'Resize Name column' });
  const box = await handle.boundingBox();
  expect(box).not.toBeNull();
  const x = box!.x + box!.width / 2;
  const y = box!.y + box!.height / 2;
  const before = await page.getByRole('group', { name: 'Name column' }).boundingBox();

  await page.mouse.move(x, y);
  await page.mouse.down();
  for (let i = 1; i <= 10; i++) await page.mouse.move(x + i * 6, y);
  const during = await page.evaluate(() => (window as unknown as { __widthWrites: string[] }).__widthWrites.length);
  await page.mouse.up();

  const writes = await page.evaluate(() => (window as unknown as { __widthWrites: string[] }).__widthWrites);
  expect(during).toBe(0);
  expect(writes).toHaveLength(1);
  const saved = JSON.parse(writes[0]) as number[];
  const after = await page.getByRole('group', { name: 'Name column' }).boundingBox();
  expect(after!.width).toBeGreaterThan(before!.width + 30);
  expect(saved.length).toBeGreaterThan(1);
});

test('keyboard resizing still saves each step', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  await openRules(page);
  const handle = page.getByRole('separator', { name: 'Resize Name column' });
  await handle.focus();
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowRight');
  const writes = await page.evaluate(() => (window as unknown as { __widthWrites: string[] }).__widthWrites.length);
  expect(writes).toBe(2);
});
