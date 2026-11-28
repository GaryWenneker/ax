/**
 * The rules-and-skills folder field on Policy sync keeps what the user types
 * in a draft (null while not editing), so a live reload of the saved settings
 * does not overwrite it, and leaving the field saves only a real change.
 */

const DEFAULT_FOLDER = '.agents';

/** The text the field shows: the draft while editing, else the saved folder. */
export function shownFolder(draft: string | null, saved: string | undefined): string {
  return draft ?? saved ?? DEFAULT_FOLDER;
}

/** The trimmed folder name to save, or null when the draft is empty or unchanged. */
export function folderToSave(draft: string | null, saved: string | undefined): string | null {
  if (draft === null) return null;
  const name = draft.trim();
  if (!name || name === (saved ?? DEFAULT_FOLDER)) return null;
  return name;
}
