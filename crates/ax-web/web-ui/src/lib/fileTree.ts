/** Browse-mode file tree loading for the Files page. */

export type BrowseTree<F, R> = {
  roots: R[];
  files: F[];
  /** Indexed files under the roots plus loose files at the top level. */
  total: number;
  /** Prefixes whose files are in `files`; `''` is the top level. */
  loaded: Set<string>;
};

export type BrowseTreeFetchers<F, R> = {
  fetchRoots: () => Promise<{ roots: R[]; files?: F[] | null }>;
  fetchPrefix: (prefix: string) => Promise<{ files: F[] }>;
};

/**
 * Fetch the roots and the files of every expanded folder, and return them as
 * one result so the caller can replace its tree state in a single update.
 * A folder that fails to load (deleted since it was expanded) is left out of
 * `loaded` instead of failing the whole refresh.
 */
export async function fetchBrowseTree<F extends { path: string }, R extends { count: number }>(
  prefixes: Iterable<string>,
  fetchers: BrowseTreeFetchers<F, R>,
): Promise<BrowseTree<F, R>> {
  const wanted = [...new Set(prefixes)].filter((p) => p !== '');
  const [rootPage, pages] = await Promise.all([
    fetchers.fetchRoots(),
    Promise.allSettled(wanted.map((prefix) => fetchers.fetchPrefix(prefix))),
  ]);
  const rootFiles = Array.isArray(rootPage.files) ? rootPage.files : [];
  const loaded = new Set<string>(['']);
  const byPath = new Map<string, F>();
  for (const f of rootFiles) byPath.set(f.path, f);
  pages.forEach((page, i) => {
    if (page.status !== 'fulfilled') return;
    loaded.add(wanted[i]);
    for (const f of page.value.files) byPath.set(f.path, f);
  });
  return {
    roots: rootPage.roots,
    files: [...byPath.values()].sort((a, b) => a.path.localeCompare(b.path)),
    total: rootPage.roots.reduce((sum, r) => sum + r.count, 0) + rootFiles.length,
    loaded,
  };
}
