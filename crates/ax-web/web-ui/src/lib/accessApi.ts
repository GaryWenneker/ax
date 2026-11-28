const OWNER_SESSION_REQUIRED = 'A local owner session is required to change access.';

/** CSRF token for access writes, fetched once per page load. */
let cachedCsrf: string | null = null;

/** Forget the cached CSRF token so the next write fetches a fresh one. */
export function clearAccessCsrf(): void {
  cachedCsrf = null;
}

async function accessCsrf(signal?: AbortSignal | null): Promise<string> {
  if (cachedCsrf) return cachedCsrf;
  const session = await fetch('/api/access/session', { cache: 'no-store', signal });
  if (!session.ok) throw new Error(OWNER_SESSION_REQUIRED);
  const body: unknown = await session.json().catch(() => null);
  const csrf = body && typeof body === 'object' ? (body as { csrf?: unknown }).csrf : undefined;
  if (typeof csrf !== 'string' || !csrf) throw new Error(OWNER_SESSION_REQUIRED);
  cachedCsrf = csrf;
  return csrf;
}

function send(url: string, init: RequestInit | undefined, csrf: string | null): Promise<Response> {
  return fetch(url, {
    ...init, cache: 'no-store',
    headers: { 'Content-Type': 'application/json', ...init?.headers, ...(csrf ? { 'x-ax-access-csrf': csrf } : {}) },
  });
}

export async function accessJson<T>(url: string, init?: RequestInit): Promise<T> {
  const mutation = Boolean(init?.method && init.method !== 'GET');
  let response = await send(url, init, mutation ? await accessCsrf(init?.signal) : null);
  if (mutation && response.status === 403) {
    // The server may have restarted with a new token: fetch it once and retry.
    clearAccessCsrf();
    response = await send(url, init, await accessCsrf(init?.signal));
    if (response.status === 403) clearAccessCsrf();
  }
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error ?? `HTTP ${response.status}`);
  return body as T;
}
