/** Background refresh for the Memory page: agents write ax.db from another process. */

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
