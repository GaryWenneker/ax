export type LspServer = {
  id: string;
  available: boolean;
  command?: string;
  path?: string | null;
  languages?: string[];
  install?: string;
};

/** Servers that are not runnable yet but have an install recipe, in list order. */
export function installableServers(servers: LspServer[]): LspServer[] {
  return servers.filter((s) => !s.available && Boolean(s.install?.trim()));
}

export type EnrichProgress = {
  running: boolean;
  done: number;
  total: number;
  server?: string | null;
  file?: string | null;
  cancelled: boolean;
};

/** Whole percent of `done` over `total`, 0 when nothing is planned, never above 100. */
export function progressPercent(done: number, total: number): number {
  if (total <= 0) return 0;
  return Math.min(100, Math.floor((done / total) * 100));
}

/** "120 / 999 · 12%" — the numbers shown next to the bar. */
export function progressLabel(p: Pick<EnrichProgress, 'done' | 'total'>): string {
  const n = (v: number) => v.toLocaleString('en-US');
  return `${n(p.done)} / ${n(p.total)} · ${progressPercent(p.done, p.total)}%`;
}

/** Validate a `GET /api/lsp/progress` body; `null` when it is not a progress snapshot. */
export function parseProgress(value: unknown): EnrichProgress | null {
  if (typeof value !== 'object' || value === null) return null;
  // Narrowed to a non-null object above; fields are checked one by one below.
  const v = value as Record<string, unknown>;
  const optText = (x: unknown) => x === undefined || x === null || typeof x === 'string';
  if (
    typeof v.running !== 'boolean' ||
    typeof v.cancelled !== 'boolean' ||
    typeof v.done !== 'number' ||
    typeof v.total !== 'number' ||
    !optText(v.server) ||
    !optText(v.file)
  ) {
    return null;
  }
  return {
    running: v.running,
    done: v.done,
    total: v.total,
    server: typeof v.server === 'string' ? v.server : null,
    file: typeof v.file === 'string' ? v.file : null,
    cancelled: v.cancelled,
  };
}

export type ErrorGroup = { message: string; files: string[] };

/** Group enrich errors (`"<path>: <message>"`) by message, in first-seen order. */
export function groupErrors(errors: string[]): ErrorGroup[] {
  const groups = new Map<string, ErrorGroup>();
  for (const error of errors) {
    const at = error.indexOf(': ');
    const file = at < 0 ? null : error.slice(0, at);
    const message = at < 0 ? error : error.slice(at + 2);
    let group = groups.get(message);
    if (!group) {
      group = { message, files: [] };
      groups.set(message, group);
    }
    if (file !== null && !group.files.includes(file)) group.files.push(file);
  }
  return [...groups.values()];
}
