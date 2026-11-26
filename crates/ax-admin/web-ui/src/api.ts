// Calls to ax-admin's own loopback API. Keys go in; they never come back.
// Every answer is checked before it is used: nothing here is cast on trust.
import { toolsFrom, type Tool } from './remoteData';
import type { SignInStatus } from './signIn';

export type { Tool } from './remoteData';

export interface Connection {
  id: string;
  name: string;
  url: string;
  auth: 'api_key' | 'oauth';
  clientId: string | null;
  write: boolean;
  active: boolean;
}

export interface RemoteInfo {
  serverName: string;
  serverVersion: string;
  protocolVersion: string;
  tools: Tool[];
}

export interface ToolResult {
  text: string;
  structuredContent: unknown;
  isError: boolean;
}

export class ApiError extends Error {
  readonly code: string;
  readonly status: number;
  constructor(status: number, code: string, message: string) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

type Fields = Record<string, unknown>;

function fields(value: unknown): Fields {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Fields) : {};
}

function text(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function unexpected(): ApiError {
  return new ApiError(0, 'unexpected_answer', 'ax-admin gave an unexpected answer. Reload this page.');
}

function connectionFrom(value: unknown): Connection {
  const c = fields(value);
  if (typeof c.id !== 'string' || typeof c.name !== 'string' || typeof c.url !== 'string') throw unexpected();
  return {
    id: c.id,
    name: c.name,
    url: c.url,
    auth: c.auth === 'oauth' ? 'oauth' : 'api_key',
    clientId: typeof c.clientId === 'string' ? c.clientId : null,
    write: c.write === true,
    active: c.active === true,
  };
}

function infoFrom(value: unknown): RemoteInfo {
  const info = fields(value);
  return {
    serverName: text(info.serverName),
    serverVersion: text(info.serverVersion),
    protocolVersion: text(info.protocolVersion),
    tools: toolsFrom(info.tools),
  };
}

let csrf = '';

async function request(method: 'GET' | 'POST' | 'PUT' | 'DELETE', path: string, body?: unknown): Promise<Fields> {
  let response: Response;
  try {
    response = await fetch(`/api${path}`, {
      method,
      headers: {
        ...(body === undefined ? {} : { 'content-type': 'application/json' }),
        ...(method === 'GET' ? {} : { 'x-ax-admin-csrf': csrf }),
      },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      cache: 'no-store',
    });
  } catch {
    throw new ApiError(0, 'admin_unreachable', 'ax-admin is not running. Start it again and reload this page.');
  }
  const data: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = fields(data);
    throw new ApiError(
      response.status,
      text(error.code) || 'error',
      text(error.error) || `Request failed (HTTP ${response.status})`,
    );
  }
  if (typeof data !== 'object' || data === null) throw unexpected();
  return fields(data);
}

export interface ConnectionList {
  connections: Connection[];
  activeId: string | null;
}

export async function listConnections(): Promise<ConnectionList> {
  const data = await request('GET', '/connections');
  if (typeof data.csrf !== 'string' || !Array.isArray(data.connections)) throw unexpected();
  csrf = data.csrf;
  return {
    connections: data.connections.map(connectionFrom),
    activeId: typeof data.activeId === 'string' ? data.activeId : null,
  };
}

type KeyAuth = { type: 'api_key'; key: string };

/** Tests a saved connection as it is saved, or a new address and key. */
export async function testConnection(input: { id: string } | { name: string; url: string; auth: KeyAuth }): Promise<RemoteInfo> {
  return infoFrom((await request('POST', '/connections/test', input)).info);
}

export async function createConnection(input: { name: string; url: string; auth: KeyAuth }): Promise<Connection> {
  return connectionFrom((await request('POST', '/connections', input)).connection);
}

export async function updateConnection(id: string, input: { name?: string; url?: string; auth?: KeyAuth }): Promise<Connection> {
  return connectionFrom((await request('PUT', `/connections/${encodeURIComponent(id)}`, input)).connection);
}

export async function removeConnection(id: string): Promise<void> {
  await request('DELETE', `/connections/${encodeURIComponent(id)}`);
}

export async function activateConnection(id: string): Promise<void> {
  await request('POST', `/connections/${encodeURIComponent(id)}/activate`, {});
}

export interface StartedSignIn {
  id: string;
  authorizationUrl: string;
}

export async function startLogin(input: { name: string; url: string; clientId: string; write: boolean; admin: boolean }): Promise<StartedSignIn> {
  const data = await request('POST', '/login', input);
  if (typeof data.id !== 'string' || typeof data.authorizationUrl !== 'string') throw unexpected();
  // Only ever opened as a web page.
  if (!/^https:\/\//i.test(data.authorizationUrl)) throw unexpected();
  return { id: data.id, authorizationUrl: data.authorizationUrl };
}

export async function loginStatus(id: string): Promise<SignInStatus> {
  const data = await request('GET', `/login/${encodeURIComponent(id)}`);
  if (data.state === 'pending' || data.state === 'ready') return { state: data.state };
  return { state: 'failed', message: text(data.message) || 'Sign-in failed.' };
}

export async function completeLogin(id: string): Promise<{ state: 'connected'; connection: Connection } | { state: 'failed'; message: string }> {
  const data = await request('POST', `/login/${encodeURIComponent(id)}/complete`, {});
  if (data.state === 'connected') return { state: 'connected', connection: connectionFrom(data.connection) };
  return { state: 'failed', message: text(data.message) || 'Sign-in failed.' };
}

export async function cancelLogin(id: string): Promise<void> {
  await request('DELETE', `/login/${encodeURIComponent(id)}`);
}

/** The active environment: what it is and which tools it offers this connection. */
export async function remoteInfo(): Promise<{ info: RemoteInfo; connection: Connection }> {
  const data = await request('GET', '/remote/info');
  return { info: infoFrom(data), connection: connectionFrom(data.connection) };
}

/** Runs a tool for `connectionId`. Refused when another connection is active by now. */
export async function callTool(connectionId: string, name: string, args: Record<string, unknown>): Promise<ToolResult> {
  const data = await request('POST', `/remote/tools/${encodeURIComponent(name)}`, { connection: connectionId, arguments: args });
  return { text: text(data.text), structuredContent: data.structuredContent ?? null, isError: data.isError === true };
}
