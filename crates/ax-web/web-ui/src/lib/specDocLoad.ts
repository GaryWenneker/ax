/** Identifies the document the Specs editor shows: a spec or a plan, by id. */
export function specDocKey(kind: 'spec' | 'plan', id: string): string {
  return `${kind}:${id}`;
}

/** The editor fields of a document that has not loaded yet. */
export interface EditorReset {
  revisions: [];
  openVersion: null;
  draftText: string;
  dirty: false;
}

/**
 * What to reset before loading `nextKey`. Opening a different document
 * clears the previous one's revisions, open version and draft, so a slow or
 * failed load never shows (or saves) its text under the new title.
 * Reloading the document already shown (after a save or an approval) keeps
 * it on screen and returns null.
 */
export function editorResetForLoad(loadedKey: string | null, nextKey: string): EditorReset | null {
  if (loadedKey === nextKey) return null;
  return { revisions: [], openVersion: null, draftText: '', dirty: false };
}
