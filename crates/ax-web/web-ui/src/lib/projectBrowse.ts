import { displayPath } from '../pathDisplay.ts';
import type { BrowseEntry } from '../workspaceApi.ts';

/** The folder the project browser shows; every field comes from one successful browse. */
export interface BrowseView {
  path: string;
  parent?: string;
  initialized: boolean;
  entries: BrowseEntry[];
}

export const EMPTY_BROWSE_VIEW: BrowseView = { path: '', parent: undefined, initialized: false, entries: [] };

export interface BrowseResponse {
  ok: boolean;
  path?: string;
  parent?: string;
  initialized?: boolean;
  entries?: BrowseEntry[];
  error?: string;
}

/**
 * What to show after a browse returns. A response that is not the latest
 * request returns null. A failure keeps the shown folder (so the footer
 * actions still aim at it) and puts the path field back to it.
 */
export function browseOutcome(
  shown: BrowseView,
  response: BrowseResponse,
  isLatest: boolean,
): { view: BrowseView; draft: string; error: string | null } | null {
  if (!isLatest) return null;
  if (response.ok && response.path) {
    const path = displayPath(response.path);
    return {
      view: {
        path,
        parent: response.parent ? displayPath(response.parent) : undefined,
        initialized: response.initialized === true,
        entries: response.entries ?? [],
      },
      draft: path,
      error: null,
    };
  }
  return { view: shown, draft: shown.path, error: response.error ?? 'Browse failed' };
}
