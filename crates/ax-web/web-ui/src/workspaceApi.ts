const BASE = '/api/workspace';

const UNREACHABLE = 'The server did not answer. Check that ax web is running and try again.';

/**
 * Send a workspace request and read its JSON reply. These helpers never throw: a failed
 * connection or a reply that is not JSON comes back as `{ ok: false, error }` so callers can
 * always clear their busy state and show the message.
 */
async function requestJson<T extends { ok: boolean; error?: string }>(
  url: string,
  init?: RequestInit,
  fallback?: Partial<T>,
): Promise<T> {
  let res: Response;
  try {
    res = await fetch(url, init);
  } catch {
    return { ...fallback, ok: false, error: UNREACHABLE } as T;
  }
  let body: unknown;
  try {
    body = await res.json();
  } catch {
    return { ...fallback, ok: false, error: `The server answered HTTP ${res.status} without a readable reply.` } as T;
  }
  if (!body || typeof body !== 'object') {
    return { ...fallback, ok: false, error: `The server answered HTTP ${res.status} without a readable reply.` } as T;
  }
  const data = body as T;
  if (!res.ok && data.ok !== false) {
    return { ...fallback, ...data, ok: false, error: data.error ?? `HTTP ${res.status}` };
  }
  return data;
}

function postJson(body: unknown): RequestInit {
  return {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  };
}

export interface RecentProject {
  path: string;
  label: string;
  last_opened: number;
  initialized: boolean;
}

export interface WorkspaceCurrent {
  path: string;
  label: string;
  initialized: boolean;
}

export interface BrowseEntry {
  name: string;
  path: string;
  is_dir: boolean;
  initialized: boolean;
}

export async function fetchWorkspaceCurrent(): Promise<{
  ok: boolean;
  workspace?: WorkspaceCurrent;
  recent?: RecentProject[];
  error?: string;
}> {
  return requestJson(`${BASE}/current`);
}

export async function fetchWorkspaceRecent(): Promise<{ ok: boolean; recent: RecentProject[]; error?: string }> {
  return requestJson<{ ok: boolean; recent: RecentProject[]; error?: string }>(`${BASE}/recent`, undefined, { recent: [] });
}

export async function browseWorkspace(path?: string): Promise<{
  ok: boolean;
  path?: string;
  parent?: string;
  initialized?: boolean;
  entries?: BrowseEntry[];
  error?: string;
}> {
  const qs = path ? `?path=${encodeURIComponent(path)}` : '';
  return requestJson(`${BASE}/browse${qs}`);
}

export async function switchWorkspace(path: string): Promise<{
  ok: boolean;
  path?: string;
  url?: string;
  reloading?: boolean;
  switched?: boolean;
  label?: string;
  needs_init?: boolean;
  error?: string;
}> {
  return requestJson(`${BASE}/switch`, postJson({ path }));
}

export interface PurgeGroup {
  id: string;
  label: string;
  detail: string;
  bytes: number;
  fileCount: number;
  defaultOn: boolean;
  empty: boolean;
}

export async function fetchPurgePlan(path: string): Promise<{
  ok: boolean;
  path?: string;
  label?: string;
  current?: boolean;
  missing?: boolean;
  note?: string;
  groups?: PurgeGroup[];
  error?: string;
}> {
  return requestJson(`${BASE}/purge-plan?path=${encodeURIComponent(path)}`);
}

export async function purgeWorkspace(
  path: string,
  groups: string[],
): Promise<{
  ok: boolean;
  path?: string;
  removed?: string[];
  errors?: string[];
  disconnected?: boolean;
  forgotRecent?: boolean;
  error?: string;
}> {
  return requestJson(`${BASE}/purge`, postJson({ path, groups }));
}

export async function mkdirWorkspace(parent: string, name: string): Promise<{
  ok: boolean;
  path?: string;
  created?: boolean;
  initialized?: boolean;
  error?: string;
}> {
  return requestJson(`${BASE}/mkdir`, postJson({ parent, name }));
}

export type WorkspaceStreamEvent =
  | { type: 'line'; text: string }
  | { type: 'done'; ok: boolean; path?: string; initialized?: boolean; error?: string };

/**
 * Run `ax init` for `path` and pass each streamed line to `onEvent`. Exactly one `done` event is
 * always sent, also when the request fails or the stream ends or breaks before the server sent
 * its own `done`, so a caller can always leave its busy state. Nothing is sent after an abort
 * through `signal`.
 */
export async function streamWorkspaceInit(
  path: string,
  onEvent: (ev: WorkspaceStreamEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  let sawDone = false;
  const emit = (ev: WorkspaceStreamEvent) => {
    if (sawDone) return;
    if (ev.type === 'done') sawDone = true;
    onEvent(ev);
  };
  let res: Response;
  try {
    res = await fetch(`${BASE}/init/stream`, { ...postJson({ path }), signal });
  } catch {
    if (!signal?.aborted) emit({ type: 'done', ok: false, error: UNREACHABLE });
    return;
  }
  if (!res.ok || !res.body) {
    emit({ type: 'done', ok: false, error: `HTTP ${res.status}` });
    return;
  }
  const reader = res.body.getReader();
  const dec = new TextDecoder();
  let buf = '';
  try {
    while (!sawDone) {
      const { done, value } = await reader.read();
      if (done) break;
      buf += dec.decode(value, { stream: true });
      const parts = buf.split('\n\n');
      buf = parts.pop() ?? '';
      for (const part of parts) {
        for (const line of part.split('\n')) {
          if (line.startsWith('data: ')) {
            try {
              emit(JSON.parse(line.slice(6)) as WorkspaceStreamEvent);
            } catch {
              /* ignore a frame that is not JSON */
            }
          }
        }
      }
    }
  } catch {
    if (signal?.aborted) return;
    emit({ type: 'done', ok: false, error: 'The connection to the server was lost before ax init finished.' });
    return;
  }
  if (!signal?.aborted) {
    emit({ type: 'done', ok: false, error: 'ax init stopped before it reported a result. Check the project and try again.' });
  }
}
