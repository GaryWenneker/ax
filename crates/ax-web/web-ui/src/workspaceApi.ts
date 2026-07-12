const BASE = '/api/workspace';

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
}> {
  const res = await fetch(`${BASE}/current`);
  return res.json();
}

export async function fetchWorkspaceRecent(): Promise<{ ok: boolean; recent: RecentProject[] }> {
  const res = await fetch(`${BASE}/recent`);
  return res.json();
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
  const res = await fetch(`${BASE}/browse${qs}`);
  return res.json();
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
  const res = await fetch(`${BASE}/switch`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path }),
  });
  return res.json();
}

export async function mkdirWorkspace(parent: string, name: string): Promise<{
  ok: boolean;
  path?: string;
  created?: boolean;
  initialized?: boolean;
  error?: string;
}> {
  const res = await fetch(`${BASE}/mkdir`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ parent, name }),
  });
  return res.json();
}

export type WorkspaceStreamEvent =
  | { type: 'line'; text: string }
  | { type: 'done'; ok: boolean; path?: string; initialized?: boolean; error?: string };

export async function streamWorkspaceInit(
  path: string,
  onEvent: (ev: WorkspaceStreamEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const res = await fetch(`${BASE}/init/stream`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path }),
    signal,
  });
  if (!res.ok || !res.body) {
    onEvent({ type: 'done', ok: false, error: `HTTP ${res.status}` });
    return;
  }
  const reader = res.body.getReader();
  const dec = new TextDecoder();
  let buf = '';
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += dec.decode(value, { stream: true });
    const parts = buf.split('\n\n');
    buf = parts.pop() ?? '';
    for (const part of parts) {
      for (const line of part.split('\n')) {
        if (line.startsWith('data: ')) {
          try {
            onEvent(JSON.parse(line.slice(6)) as WorkspaceStreamEvent);
          } catch {
            /* ignore */
          }
        }
      }
    }
  }
}
