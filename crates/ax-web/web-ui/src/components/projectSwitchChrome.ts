export type SwitcherChrome<T> = {
  menuOpen: boolean;
  project: T;
};

/** Trash opens Remove ax data and drops the Switch project list. */
export function chromeAfterTrashClick<T extends { path: string }>(project: T): SwitcherChrome<T> {
  return { menuOpen: false, project };
}
