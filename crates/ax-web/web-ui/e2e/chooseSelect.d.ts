import type { Locator } from '@playwright/test';
/** Open a SettingsSelect and choose an option by its value. */
export declare function chooseSelect(field: Locator, value: string): Promise<void>;
