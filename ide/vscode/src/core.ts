import * as path from 'node:path';

function checkPort(port: number): number {
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error(`ax.webPort must be a port number between 1 and 65535, got ${port}`);
  }
  return port;
}

export function commandCenterUrl(port: number): string {
  return `http://127.0.0.1:${checkPort(port)}/?embed=1`;
}

/** The webview only frames the local Command Center; it runs no script of its own. */
export function webviewHtml(port: number, nonce: string): string {
  const origin = `http://127.0.0.1:${checkPort(port)}`;
  const csp = `default-src 'none'; frame-src ${origin}; style-src 'nonce-${nonce}';`;
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy" content="${csp}">
<style nonce="${nonce}">html,body{margin:0;height:100%;background:#1e1e1e}iframe{position:absolute;inset:0;border:0;width:100%;height:100%}</style>
</head>
<body><iframe src="${commandCenterUrl(port)}" title="ax Command Center"></iframe></body>
</html>`;
}

export interface PanelLike {
  reveal(): void;
  reload?(): void;
  onDispose(fn: () => void): void;
}

/** Keeps a single Command Center panel: a second open reveals it instead of opening another. */
export class PanelHost {
  private readonly create: () => PanelLike;
  private panel: PanelLike | undefined;

  constructor(create: () => PanelLike) {
    this.create = create;
  }

  open(): void {
    if (this.panel) {
      this.panel.reload?.();
      this.panel.reveal();
      return;
    }
    const panel = this.create();
    panel.onDispose(() => {
      if (this.panel === panel) this.panel = undefined;
    });
    this.panel = panel;
  }
}

export const NO_WORKSPACE_FOLDER = 'Open a folder to choose its ax project.';
export const STATUS_BAR_TEXT = '$(graph) ax';
export const STATUS_BAR_TOOLTIP = 'Open ax Command Center for this workspace';

export interface RecentProject {
  path: string;
  label: string;
  initialized: boolean;
}

export interface PopoutRow {
  kind: 'open' | 'switch';
  label: string;
  detail: string;
  path: string;
}

export function sameProjectPath(a: string, b: string): boolean {
  const norm = (value: string) => {
    const abs = path.resolve(value);
    return process.platform === 'win32' ? abs.toLowerCase() : abs;
  };
  return norm(a) === norm(b);
}

function isInside(folder: string, file: string): boolean {
  const rel = path.relative(path.resolve(folder), path.resolve(file));
  if (rel === '') return true;
  if (rel === '..' || rel.startsWith(`..${path.sep}`)) return false;
  return !path.isAbsolute(rel);
}

function folderForActiveFile(folders: readonly string[], activeFile: string | undefined): string | undefined {
  if (!activeFile) return undefined;
  let best: string | undefined;
  let bestLen = -1;
  for (const folder of folders) {
    if (!isInside(folder, activeFile)) continue;
    const len = path.resolve(folder).length;
    if (len > bestLen) {
      best = folder;
      bestLen = len;
    }
  }
  return best;
}

function nearestProjectRoot(start: string, hasAxDb: (root: string) => boolean): string | undefined {
  let current = path.resolve(start);
  const stop = path.parse(current).root;
  for (;;) {
    if (hasAxDb(current)) return current;
    if (current === stop) return undefined;
    const parent = path.dirname(current);
    if (parent === current) return undefined;
    current = parent;
  }
}

export function workspaceProjectPath(
  folders: readonly string[],
  activeFile: string | undefined,
  hasAxDb: (root: string) => boolean,
): string | undefined {
  if (folders.length === 0) return undefined;
  const chosen = folderForActiveFile(folders, activeFile) ?? folders[0];
  if (!chosen) return undefined;
  return nearestProjectRoot(chosen, hasAxDb) ?? path.resolve(chosen);
}

export function popoutRows(workspacePath: string, recent: readonly RecentProject[]): PopoutRow[] {
  const rows: PopoutRow[] = [
    { kind: 'open', label: 'Open Command Center', detail: workspacePath, path: workspacePath },
  ];
  for (const project of recent) {
    if (!project.initialized) continue;
    if (sameProjectPath(project.path, workspacePath)) continue;
    rows.push({ kind: 'switch', label: project.label, detail: project.path, path: project.path });
  }
  return rows;
}

export function launchArgs(port: number, projectPath: string): string[] {
  return ['web', '--port', String(checkPort(port)), projectPath];
}

export function rowAction(row: PopoutRow): { openPanel: true; switchPath?: string } {
  if (row.kind === 'switch') return { openPanel: true, switchPath: row.path };
  return { openPanel: true };
}

export type PrepareResult =
  | { ok: true; rows: PopoutRow[]; switched: boolean }
  | { ok: false; message: string; kind: 'folder' | 'start' | 'hub' };

const HUB_TIMEOUT_MS = 5_000;

function hubUrl(port: number, suffix: string): string {
  return `http://127.0.0.1:${checkPort(port)}${suffix}`;
}

function hubSignal(): AbortSignal {
  return AbortSignal.timeout(HUB_TIMEOUT_MS);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value);
}

function errorMessage(value: unknown, fallback: string): string {
  const rec = isRecord(value) ? value : undefined;
  return typeof rec?.error === 'string' && rec.error.length > 0 ? rec.error : fallback;
}

function recentProjects(value: unknown): RecentProject[] {
  if (!Array.isArray(value)) return [];
  const out: RecentProject[] = [];
  for (const item of value) {
    const rec = isRecord(item) ? item : undefined;
    if (!rec || typeof rec.path !== 'string' || typeof rec.label !== 'string') continue;
    out.push({ path: rec.path, label: rec.label, initialized: rec.initialized === true });
  }
  return out;
}

async function readJson(res: Response): Promise<unknown> {
  try {
    return await res.json();
  } catch {
    return undefined;
  }
}

export async function switchProject(
  port: number,
  projectPath: string,
  fetchImpl: typeof fetch,
): Promise<{ ok: true } | { ok: false; message: string }> {
  let res: Response;
  try {
    res = await fetchImpl(hubUrl(port, '/api/workspace/switch'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json; charset=utf-8' },
      body: JSON.stringify({ path: projectPath }),
      signal: hubSignal(),
    });
  } catch (e) {
    return { ok: false, message: e instanceof Error ? e.message : String(e) };
  }
  const body = await readJson(res);
  if (isRecord(body) && body.ok === true) return { ok: true };
  return { ok: false, message: errorMessage(body, 'Command Center did not switch project.') };
}

async function readCurrent(
  port: number,
  fetchImpl: typeof fetch,
): Promise<{ ok: true; path?: string; recent: RecentProject[] } | { ok: false; message: string }> {
  let res: Response;
  try {
    res = await fetchImpl(hubUrl(port, '/api/workspace/current'), { signal: hubSignal() });
  } catch (e) {
    return { ok: false, message: e instanceof Error ? e.message : String(e) };
  }
  const body = await readJson(res);
  if (!isRecord(body) || body.ok !== true) {
    return { ok: false, message: errorMessage(body, 'Command Center did not report the current project.') };
  }
  const workspace = isRecord(body.workspace) ? body.workspace : undefined;
  const currentPath = typeof workspace?.path === 'string' ? workspace.path : undefined;
  return { ok: true, path: currentPath, recent: recentProjects(body.recent) };
}

export async function preparePopout(input: {
  port: number;
  folders: readonly string[];
  activeFile?: string;
  hasAxDb: (root: string) => boolean;
  ensureServer: (port: number, projectPath: string) => Promise<void>;
  fetchImpl: typeof fetch;
}): Promise<PrepareResult> {
  const workspacePath = workspaceProjectPath(input.folders, input.activeFile, input.hasAxDb);
  if (!workspacePath) return { ok: false, kind: 'folder', message: NO_WORKSPACE_FOLDER };
  try {
    await input.ensureServer(input.port, workspacePath);
  } catch (e) {
    return { ok: false, kind: 'start', message: e instanceof Error ? e.message : String(e) };
  }
  const current = await readCurrent(input.port, input.fetchImpl);
  if (!current.ok) return { ok: false, kind: 'hub', message: current.message };
  const already = current.path !== undefined && sameProjectPath(current.path, workspacePath);
  if (!already) {
    const switched = await switchProject(input.port, workspacePath, input.fetchImpl);
    if (!switched.ok) return { ok: false, kind: 'hub', message: switched.message };
  }
  return { ok: true, switched: !already, rows: popoutRows(workspacePath, current.recent) };
}
