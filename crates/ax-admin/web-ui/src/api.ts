// Calls to ax-admin's own loopback API. Keys go in; they never come back.

export interface Connection {
  id: string;
  name: string;
  url: string;
  auth: 'api_key' | 'oauth';
  clientId: string | null;
  write: boolean;
  active: boolean;
}

export interface Tool {
  name: string;
  description?: string;
  inputSchema?: unknown;
  annotations?: { readOnlyHint?: boolean };
}

export interface RemoteInfo {
  serverName: string;
  serverVersion: string;
  protocolVersion: string;
  tools: Tool[];
  connection?: Connection;
}

export interface ToolResult {
  text: string;
  structuredContent: unknown;
  isError: boolean;
}

export class ApiError extends Error {
  code: string;
  status: number;
  constructor(status: number, code: string, message: string) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

let csrf = '';

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`/api${path}`, {
      method,
      headers: {
        ...(body === undefined ? {} : { 'content-type': 'application/json' }),
        ...(method === 'GET' ? {} : { 'x-ax-admin-csrf': csrf }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
      cache: 'no-store',
    });
  } catch {
    throw new ApiError(0, 'admin_unreachable', 'ax-admin is not running. Start it again and reload this page.');
  }
  const data: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = (data ?? {}) as { code?: string; error?: string };
    throw new ApiError(response.status, error.code ?? 'error', error.error ?? `Request failed (HTTP ${response.status})`);
  }
  return data as T;
}

export interface ConnectionList {
  connections: Connection[];
  activeId: string | null;
}

export async function listConnections(): Promise<ConnectionList> {
  const data = await request<ConnectionList & { csrf: string }>('GET', '/connections');
  csrf = data.csrf;
  return { connections: data.connections, activeId: data.activeId };
}

type KeyAuth = { type: 'api_key'; key: string };

export function testConnection(input: { id?: string; name?: string; url?: string; auth?: KeyAuth }) {
  return request<{ ok: true; info: RemoteInfo }>('POST', '/connections/test', input);
}

export function createConnection(input: { name: string; url: string; auth: KeyAuth }) {
  return request<{ connection: Connection; info: RemoteInfo }>('POST', '/connections', input);
}

export function updateConnection(id: string, input: { name?: string; url?: string; auth?: KeyAuth }) {
  return request<{ connection: Connection }>('PUT', `/connections/${encodeURIComponent(id)}`, input);
}

export function removeConnection(id: string) {
  return request<{ removed: string; activeId: string | null }>('DELETE', `/connections/${encodeURIComponent(id)}`);
}

export function activateConnection(id: string) {
  return request<{ activeId: string }>('POST', `/connections/${encodeURIComponent(id)}/activate`, {});
}

export function startLogin(input: { name: string; url: string; clientId: string; write: boolean }) {
  return request<{ id: string; authorizationUrl: string }>('POST', '/login', input);
}

export interface LoginStatus {
  state: 'pending' | 'connected' | 'failed';
  message?: string;
  connection?: Connection;
}

export function loginStatus(id: string) {
  return request<LoginStatus>('GET', `/login/${encodeURIComponent(id)}`);
}

export function cancelLogin(id: string) {
  return request<{ cancelled: true }>('DELETE', `/login/${encodeURIComponent(id)}`);
}

export function remoteInfo() {
  return request<RemoteInfo>('GET', '/remote/info');
}

export function callTool(name: string, args: Record<string, unknown> = {}) {
  return request<ToolResult>('POST', `/remote/tools/${encodeURIComponent(name)}`, { arguments: args });
}
