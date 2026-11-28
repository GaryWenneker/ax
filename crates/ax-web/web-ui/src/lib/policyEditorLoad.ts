/**
 * Load state shared by the rule and skill editors. An existing item that
 * failed to load must not show an editable blank form: saving it would
 * overwrite the real item with empty metadata and body.
 */
export type PolicyEditorLoad = 'loading' | 'failed' | 'ready';

/** A frontmatter list as the comma-separated text of its input. A missing list is empty. */
export function joinPolicyList(values: readonly string[] | null | undefined): string {
  return (values ?? []).join(', ');
}

/** Save is only offered once the item is loaded (or new) and no save is running. */
export function policyEditorCanSave({ load, saving }: { load: PolicyEditorLoad; saving: boolean }): boolean {
  return load === 'ready' && !saving;
}

/** What the editor body shows for a load state. */
export function policyEditorView(load: PolicyEditorLoad): 'spinner' | 'retry' | 'form' {
  if (load === 'loading') return 'spinner';
  if (load === 'failed') return 'retry';
  return 'form';
}
