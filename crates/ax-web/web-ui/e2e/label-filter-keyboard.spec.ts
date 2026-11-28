import { expect, test, type Page } from '@playwright/test';

import { mockApi, policyRow } from './fixtures';

async function openRules(page: Page) {
  await mockApi(page, {
    '/api/policy/rules': { rules: [policyRow()] },
    '/api/policy/settings': { storage: 'files' },
    '/api/policy/rules/catalog/audits': [],
  });
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto('/policy/rules');
  await page.getByRole('button', { name: 'Show filters' }).click();
  const input = page.getByRole('combobox', { name: 'Filter rules by labels and text' });
  await expect(input).toBeVisible();
  return input;
}

const chips = (page: Page) => page.locator('.label-ac-chip');

test('Tab through the label filter moves on without adding a label', async ({ page, isMobile }) => {
  test.skip(isMobile, 'keyboard');
  const input = await openRules(page);
  await input.focus();
  await expect(page.locator('.label-ac-menu')).toBeVisible();
  await page.keyboard.press('Tab');
  await expect(chips(page)).toHaveCount(0);
  await expect(input).not.toBeFocused();
  await expect(page.locator('.label-ac-menu')).toHaveCount(0);
});

test('Tab picks the option the user moved to', async ({ page, isMobile }) => {
  test.skip(isMobile, 'keyboard');
  const input = await openRules(page);
  await input.focus();
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Tab');
  await expect(chips(page)).toHaveText(['workflow×']);
});

test('Tab after typing picks the first match', async ({ page, isMobile }) => {
  test.skip(isMobile, 'keyboard');
  const input = await openRules(page);
  await input.focus();
  await input.pressSequentially('gra');
  await page.keyboard.press('Tab');
  await expect(chips(page)).toHaveText(['graph×']);
});

test('Enter still picks the first suggestion and the active option is announced', async ({ page, isMobile }) => {
  test.skip(isMobile, 'keyboard');
  const input = await openRules(page);
  await input.focus();
  const active = await input.getAttribute('aria-activedescendant');
  expect(active).toBeTruthy();
  await expect(page.locator(`[id="${active}"]`)).toHaveAttribute('aria-selected', 'true');
  await expect(page.locator(`[id="${active}"]`)).toContainText('graph');
  await page.keyboard.press('ArrowDown');
  const moved = await input.getAttribute('aria-activedescendant');
  expect(moved).not.toBe(active);
  await expect(page.locator(`[id="${moved}"]`)).toContainText('workflow');
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('Enter');
  await expect(chips(page)).toHaveText(['graph×']);
});
