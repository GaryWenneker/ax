import type { Locator } from '@playwright/test';

/** Open a SettingsSelect and choose an option by its value. */
export async function chooseSelect(field: Locator, value: string) {
  const label = await field.evaluate((el) => (el as HTMLElement).getAttribute('aria-label') ?? '');
  await field.click();
  await field.page().getByRole('listbox', { name: label, exact: true }).locator(`[data-value="${value}"]`).click();
}
