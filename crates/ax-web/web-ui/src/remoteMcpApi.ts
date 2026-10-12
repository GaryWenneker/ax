export type RemoteClient = 'cursor' | 'vscode' | 'cline' | 'claudeCode' | 'chatgpt' | 'axProxy';
export type RemoteTransport = 'direct' | 'proxy';
export type LocalClient = 'cursor' | 'vscode' | 'cline' | 'claudeCode' | 'other';
export interface LocalExport { workspaceId: string; client: LocalClient; target: 'thisComputer' | 'projectFolder'; content: string; fileName: string; destination: string; instructions: string[]; containsSecrets: false; containsLocalPaths: boolean; portable: boolean }
export interface ConnectionExport { workspaceId: string; publicationId: string; configFingerprint: string; content: string; containsSecrets: false; requiresClientId: boolean }
export interface Publication {
  id: string; publisherId: string; publisherLabel: string; projectLabel: string;
  endpointUrl: string; publicOrigin: string; oauthIssuer: string; resource: string;
  remoteProjectId: string; configFingerprint: string;
  runtimeStatus: 'running' | 'stopped' | 'unknown'; lastHeartbeatAt: number | null;
}
export interface PublicationList { workspaceId: string; projectLabel: string; csrf: string; publications: Publication[] }
export interface RemoteExport {
  workspaceId: string; publicationId: string; configFingerprint: string; client: RemoteClient;
  format: 'json' | 'url'; fileName: string | null; content: string; instructions: string[];
  requirements: string[]; loginCommand: string | null; containsSecrets: false;
}
function record(value: unknown): value is Record<string, unknown> { return typeof value === 'object' && value !== null && !Array.isArray(value); }
function strings(value: unknown): value is string[] { return Array.isArray(value) && value.every(v => typeof v === 'string'); }
export function isRemoteClient(value: unknown): value is RemoteClient { return typeof value === 'string' && ['cursor', 'vscode', 'cline', 'claudeCode', 'chatgpt', 'axProxy'].includes(value); }
function validPublication(value: unknown): value is Publication {
  return record(value) && ['id', 'publisherId', 'publisherLabel', 'projectLabel', 'endpointUrl', 'publicOrigin', 'oauthIssuer', 'resource', 'remoteProjectId', 'configFingerprint'].every(k => typeof value[k] === 'string')
    && ['running', 'stopped', 'unknown'].includes(String(value.runtimeStatus)) && (value.lastHeartbeatAt === null || typeof value.lastHeartbeatAt === 'number');
}
function validResponse(path: string, value: unknown): boolean {
  if (!record(value)) return false;
  if (path === '/') return typeof value.workspaceId === 'string' && typeof value.csrf === 'string' && typeof value.projectLabel === 'string' && Array.isArray(value.publications) && value.publications.every(validPublication);
  if (path === '/host') return typeof value.workspaceId === 'string' && (value.host === null || (record(value.host) && ['running', 'stopped', 'error'].includes(String(value.host.status)) && ['publicUrl', 'localUrl'].every(k => typeof value.host === 'object' && value.host !== null && typeof (value.host as Record<string, unknown>)[k] === 'string') && typeof value.host.builtInOAuth === 'boolean' && typeof value.host.autoStart === 'boolean' && (value.host.message === null || typeof value.host.message === 'string')));
  if (path === '/host/authorizations') return typeof value.workspaceId === 'string' && Array.isArray(value.requests) && value.requests.every(r => record(r) && ['id', 'clientName', 'scope', 'status', 'redirectUri'].every(k => typeof r[k] === 'string') && typeof r.expires === 'number' && typeof r.revoked === 'boolean');
  if (path.startsWith('/host/')) return typeof value.workspaceId === 'string' && value.updated === true;
  if (path === '/local/export') return ['workspaceId', 'content', 'fileName', 'destination'].every(k => typeof value[k] === 'string') && ['cursor', 'vscode', 'cline', 'claudeCode', 'other'].includes(String(value.client)) && ['thisComputer', 'projectFolder'].includes(String(value.target)) && strings(value.instructions) && value.containsSecrets === false && typeof value.containsLocalPaths === 'boolean' && typeof value.portable === 'boolean';
  if (path.endsWith('/connection')) return ['workspaceId', 'publicationId', 'configFingerprint', 'content'].every(k => typeof value[k] === 'string') && value.containsSecrets === false && typeof value.requiresClientId === 'boolean';
  if (path.endsWith('/export')) return ['workspaceId', 'publicationId', 'configFingerprint', 'content'].every(k => typeof value[k] === 'string') && isRemoteClient(value.client)
    && ['json', 'url'].includes(String(value.format)) && (value.fileName === null || typeof value.fileName === 'string') && strings(value.instructions) && strings(value.requirements)
    && (value.loginCommand === null || typeof value.loginCommand === 'string') && value.containsSecrets === false;
  if (path.endsWith('/check')) return ['workspaceId', 'publicationId', 'configFingerprint', 'message'].every(k => typeof value[k] === 'string');
  return typeof value.workspaceId === 'string' && (value.registered === true || value.removed === true);
}
export class RemoteMcpError extends Error { constructor(message: string, public status: number) { super(message); } }
export async function publicationRequest<T>(path = '/', options?: { method?: 'POST' | 'DELETE'; body?: unknown; csrf?: string; signal?: AbortSignal }): Promise<T> {
  const response = await fetch(`/api/remote-mcp/publications${path}`, {
    method: options?.method ?? 'GET', signal: options?.signal,
    headers: options?.method ? { 'Content-Type': 'application/json', 'X-Ax-Csrf': options.csrf ?? '' } : {},
    body: options?.body === undefined ? undefined : JSON.stringify(options.body),
  });
  const data: unknown = await response.json().catch(() => ({ error: `Request failed (${response.status})` }));
  if (!response.ok) throw new RemoteMcpError(record(data) && typeof data.error === 'string' ? data.error : 'Project publication request failed', response.status);
  if (!validResponse(path, data)) throw new RemoteMcpError('Invalid project publication response. Refresh Settings.', 502);
  // Each supported route above validates the corresponding local Rust API contract.
  return data as T;
}
