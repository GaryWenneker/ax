import type { AgentTargetStatus } from './agentApi';

export type IdeState = 'connected' | 'found' | 'missing';

export interface IdeReport {
  id: string;
  display_name: string;
  files: string[];
  notes: string[];
}

export function ideState(t: AgentTargetStatus): IdeState {
  if (t.configured) return 'connected';
  return t.detected ? 'found' : 'missing';
}

export const IDE_STATE_LABEL: Record<IdeState, string> = {
  connected: 'Connected',
  found: 'Found',
  missing: 'Not found',
};

/** One line per IDE: what changed on disk, then any follow-up note. */
export function ideResultText(verb: 'Connected' | 'Disconnected', reports: IdeReport[]): string {
  if (reports.length === 0) return 'Nothing changed.';
  return reports
    .map((r) => {
      const files = r.files.length ? r.files.join(', ') : 'already up to date';
      return [`${verb} ${r.display_name}: ${files}.`, ...r.notes].join(' ');
    })
    .join('\n');
}

/** Ids of IDEs that are on this machine but not connected yet. */
export function foundNotConnected(targets: AgentTargetStatus[]): string[] {
  return targets.filter((t) => ideState(t) === 'found').map((t) => t.id);
}

/** Command Center badge; `null` for terminal agents, which have no panel. */
export function panelLabel(t: AgentTargetStatus): string | null {
  if (t.panel == null) return null;
  return t.panel ? 'Panel' : 'No panel';
}

export type LoadState = 'loading' | 'ready' | 'error';

export function connectAllText(load: LoadState, found: number): string {
  if (load === 'loading') return 'Loading IDEs…';
  if (load === 'error') return 'Could not load IDEs.';
  return found ? `${found} found but not connected.` : 'Every IDE found here is connected.';
}

async function post(path: string, targets: string[]): Promise<IdeReport[]> {
  const res = await fetch(`/api/agent/${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ targets }),
  });
  const data = (await res.json().catch(() => null)) as
    | { ok?: boolean; error?: string; reports?: IdeReport[] | number; results?: IdeReport[] }
    | null;
  if (!res.ok || !data?.ok) throw new Error(data?.error ?? `HTTP ${res.status}`);
  return data.results ?? (Array.isArray(data.reports) ? data.reports : []);
}

export const connectIdes = (targets: string[]) => post('install', targets);
export const disconnectIdes = (targets: string[]) => post('uninstall', targets);
