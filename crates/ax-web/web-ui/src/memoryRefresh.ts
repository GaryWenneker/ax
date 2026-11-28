/** Loading and background refresh for the Memory page: agents write ax.db from another process. */

export const MEMORY_REFRESH_MS = 5000;

type RowStamp = { id: string; updated_at: number; enabled?: boolean; score?: number };

/** True when a refetched list differs from what is on screen (order, content, toggle, score). */
export function memoryListChanged(prev: RowStamp[], next: RowStamp[]): boolean {
  if (prev.length !== next.length) return true;
  return next.some((n, i) => {
    const p = prev[i];
    return p.id !== n.id || p.updated_at !== n.updated_at || p.enabled !== n.enabled || p.score !== n.score;
  });
}

/** Rows the vault loads at first and with each "Load more". */
export const MEMORY_PAGE = 200;

/** The most rows the server returns for one list request. */
export const MEMORY_SERVER_MAX = 500;

type Page<R> = { memories: R[]; total: number };

/**
 * The newest `count` memories, fetched in pages the server accepts. A refresh
 * uses it to reload every row the vault has loaded so far.
 */
export async function fetchMemoryWindow<R>(
  count: number,
  fetchPage: (limit: number, offset: number) => Promise<Page<R>>,
): Promise<Page<R>> {
  const want = Math.max(1, count);
  const memories: R[] = [];
  let total = 0;
  while (memories.length < want) {
    const limit = Math.min(MEMORY_SERVER_MAX, want - memories.length);
    const page = await fetchPage(limit, memories.length);
    total = page.total;
    memories.push(...page.memories);
    if (page.memories.length < limit) break;
  }
  return { memories, total };
}

/** `prev` followed by the rows of `next` it does not hold yet (a new memory shifts the pages). */
export function appendMemoryPage<R extends { id: string }>(prev: R[], next: R[]): R[] {
  const seen = new Set(prev.map((m) => m.id));
  return prev.concat(next.filter((m) => !seen.has(m.id)));
}

/** Memories per source: git capture, CLI turns, and everything else (manual or agent). */
export function memorySourceCounts(rows: { source?: string }[]): { git: number; cliTurn: number; manual: number } {
  let git = 0;
  let cliTurn = 0;
  for (const m of rows) {
    if (m.source === 'git') git += 1;
    else if (m.source === 'cli-turn') cliTurn += 1;
  }
  return { git, cliTurn, manual: rows.length - git - cliTurn };
}

/**
 * The memory the blade shows: the selected row when it is listed, else the
 * copy fetched by id (a deep link or a related memory outside the loaded rows).
 */
export function pickSelectedMemory<R extends { id: string }>(shown: R[], selectedId: string, fetched: R | null): R | null {
  if (!selectedId) return null;
  return shown.find((m) => m.id === selectedId) ?? (fetched?.id === selectedId ? fetched : null);
}
