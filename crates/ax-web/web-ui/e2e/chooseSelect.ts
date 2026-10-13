import type { Locator } from '@playwright/test';

/** Open a SettingsSelect and choose an option by its value. */
export async function chooseSelect(field: Locator, value: string) {
  await field.click();
  const label = (await field.getAttribute('aria-label')) ?? '';
  await field.page().getByRole('listbox', { name: label, exact: true }).locator(`[data-value="${value}"]`).click();
}
