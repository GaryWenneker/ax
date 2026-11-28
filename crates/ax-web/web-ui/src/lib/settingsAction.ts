export type SettingsActionResult = { ok: boolean; error: string | null };

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Run a settings action, then refresh the page's data.
 * `ok` says whether the action itself succeeded; a refresh failure after a
 * successful action is reported in `error` without turning `ok` false.
 */
export async function runSettingsAction(
  action: () => Promise<unknown>,
  reload: () => Promise<void>,
): Promise<SettingsActionResult> {
  try {
    await action();
  } catch (error) {
    return { ok: false, error: messageOf(error) };
  }
  try {
    await reload();
  } catch (error) {
    return { ok: true, error: `Done, but the page could not refresh: ${messageOf(error)}` };
  }
  return { ok: true, error: null };
}
