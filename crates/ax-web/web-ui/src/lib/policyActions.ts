/**
 * Run a policy list action (delete a global copy, move a row), show its error
 * if it fails, and always reload the list afterwards so the rows match the
 * server even when the action stopped part-way.
 */
export async function runThenReload(
  action: () => Promise<unknown>,
  reload: () => void,
  onError: (message: string) => void,
  fallback: string,
): Promise<void> {
  try {
    await action();
  } catch (e) {
    onError(e instanceof Error ? e.message : fallback);
  } finally {
    reload();
  }
}
