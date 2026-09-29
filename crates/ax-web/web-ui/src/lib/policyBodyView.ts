export type BodyView = 'markdown' | 'wysiwyg' | 'preview';

export const BODY_VIEW_KEY = 'ax-web-policy-body-view';
export const POLICY_LIST_MIN = 240;
export const POLICY_LIST_MAX = 480;
export const POLICY_LIST_DEFAULT = 280;

export function loadBodyView(storage: Pick<Storage, 'getItem'> = localStorage): BodyView {
  const stored = storage.getItem(BODY_VIEW_KEY);
  return stored === 'markdown' || stored === 'wysiwyg' ? stored : 'preview';
}

export function saveBodyView(view: BodyView, storage: Pick<Storage, 'setItem'> = localStorage): void {
  storage.setItem(BODY_VIEW_KEY, view);
}

export function clampPolicyListWidth(px: number): number {
  if (Number.isNaN(px)) return POLICY_LIST_DEFAULT;
  return Math.min(POLICY_LIST_MAX, Math.max(POLICY_LIST_MIN, px));
}
