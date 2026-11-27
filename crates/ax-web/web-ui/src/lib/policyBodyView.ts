import { loadString, saveString } from './uiStorage.ts';

export type BodyView = 'markdown' | 'wysiwyg' | 'preview' | 'split';

/** Key under the shared `ax-web-` prefix of lib/uiStorage. */
const BODY_VIEW_SUFFIX = 'policy-body-view';
export const BODY_VIEW_KEY = `ax-web-${BODY_VIEW_SUFFIX}`;
export const POLICY_LIST_MIN = 240;
export const POLICY_LIST_MAX = 1600;
export const POLICY_LIST_DEFAULT = 480;

function asBodyView(stored: string | null): BodyView {
  return stored === 'markdown' || stored === 'wysiwyg' || stored === 'split' ? stored : 'preview';
}

/** Saved body view, or `preview` when nothing is saved or storage is blocked. `storage` is for tests. */
export function loadBodyView(storage?: Pick<Storage, 'getItem'>): BodyView {
  if (!storage) return asBodyView(loadString(BODY_VIEW_SUFFIX, '') || null);
  try {
    return asBodyView(storage.getItem(BODY_VIEW_KEY));
  } catch {
    return 'preview';
  }
}

/** Remember the body view; a blocked or full storage drops it. `storage` is for tests. */
export function saveBodyView(view: BodyView, storage?: Pick<Storage, 'setItem'>): void {
  if (!storage) {
    saveString(BODY_VIEW_SUFFIX, view);
    return;
  }
  try {
    storage.setItem(BODY_VIEW_KEY, view);
  } catch {
    /* blocked or full */
  }
}

export function clampPolicyListWidth(px: number): number {
  if (Number.isNaN(px)) return POLICY_LIST_DEFAULT;
  return Math.min(POLICY_LIST_MAX, Math.max(POLICY_LIST_MIN, px));
}
