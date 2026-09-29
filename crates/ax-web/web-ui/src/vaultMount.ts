export type MountOs = 'mac' | 'windows' | 'linux';

export interface MountStatus {
  os: MountOs;
  mounted: boolean;
  mountPoint: string | null;
  url: string;
  name: string;
  autostart: boolean;
  commands?: string[][];
}

export interface MountError {
  message: string;
  command: string | null;
}

const BASE = '/api/dav/mount';

export function openLabel(os: MountOs): string {
  if (os === 'mac') return 'Open in Finder';
  if (os === 'windows') return 'Open in Explorer';
  return 'Open in Files';
}

export function mountSummary(s: MountStatus): string {
  if (s.mounted && s.mountPoint) return `Connected as "${s.name}" at ${s.mountPoint}`;
  if (s.mountPoint) return `Not connected (last used ${s.mountPoint})`;
  return 'Not connected';
}

export function validMountName(name: string): boolean {
  return /^[A-Za-z0-9 _-]{1,32}$/.test(name);
}

export function mountError(status: number, body: unknown): MountError {
  const b = (body && typeof body === 'object' ? body : {}) as { error?: unknown; command?: unknown };
  const command = typeof b.command === 'string' ? b.command : null;
  if (typeof b.error === 'string') return { message: b.error, command };
  if (status === 403) {
    return {
      message: `Only the local browser on this machine can connect the vault (HTTP ${status}).`,
      command,
    };
  }
  return { message: `HTTP ${status}`, command };
}

async function call(method: string, path = '', body?: unknown): Promise<MountStatus> {
  const res = await fetch(`${BASE}${path}`, {
    method,
    headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data: unknown = await res.json().catch(() => null);
  if (!res.ok) throw mountError(res.status, data);
  return data as MountStatus;
}

export const fetchMountStatus = () => call('GET');
export const connectVault = (name: string, autostart: boolean) =>
  call('POST', '', { name, autostart });
export const disconnectVault = () => call('DELETE');
export const openVault = () => call('POST', '/open');

export interface FolderSync {
  atMs: number;
  added: number;
  updated: number;
  removed: number;
  skipped: number;
}

export interface VaultFolder {
  name: string;
  path: string;
  index: boolean;
  lastSync?: FolderSync | null;
}

export function folderSyncSummary(f: VaultFolder): string {
  if (!f.index) return 'Not indexed';
  const s = f.lastSync;
  if (!s) return 'Not synced yet';
  return `Synced: ${s.added} added, ${s.updated} updated, ${s.removed} removed, ${s.skipped} skipped`;
}

const FOLDERS = '/api/vault/folders';

export interface VaultFolderList {
  folders: VaultFolder[];
  readonly: boolean;
}

async function folderCall<T>(method: string, path = '', body?: unknown): Promise<T> {
  const res = await fetch(`${FOLDERS}${path}`, {
    method,
    headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data: unknown = await res.json().catch(() => null);
  if (!res.ok) throw new Error(mountError(res.status, data).message);
  return data as T;
}

export const fetchVaultFolders = () => folderCall<VaultFolderList>('GET');
export const saveVaultFolders = (folders: Pick<VaultFolder, 'name' | 'path' | 'index'>[]) =>
  folderCall<VaultFolderList>('PUT', '', folders);
export const syncVaultFolder = (name: string) =>
  folderCall<{ report: FolderSync }>('POST', `/${encodeURIComponent(name)}/sync`);
export const removeVaultFolder = (name: string) =>
  folderCall<VaultFolderList>('DELETE', `/${encodeURIComponent(name)}`);

/** A valid folder name from the last part of `path`, or '' when none can be made. */
export function folderNameFromPath(path: string): string {
  const last = path.split(/[\\/]/).filter(Boolean).pop() ?? '';
  if (/^[A-Za-z]:$/.test(last)) return '';
  const name = last.replace(/[^A-Za-z0-9 _-]/g, '-').slice(0, 32).trim();
  return validMountName(name) ? name : '';
}

/** Opens the OS folder dialog through ax. `undefined` means no dialog exists on this system. */
export async function pickFolderPath(): Promise<string | null | undefined> {
  const res = await fetch('/api/vault/folder-picker', { method: 'POST' });
  const data: unknown = await res.json().catch(() => null);
  if (res.status === 501) return undefined;
  if (!res.ok) throw new Error(mountError(res.status, data).message);
  return (data as { path: string | null }).path;
}
