/** Open a SettingsSelect and choose an option by its value. */
export async function chooseSelect(field, value) {
    const label = await field.evaluate((el) => el.getAttribute('aria-label') ?? '');
    await field.click();
    await field.page().getByRole('listbox', { name: label, exact: true }).locator(`[data-value="${value}"]`).click();
}
//# sourceMappingURL=chooseSelect.js.map