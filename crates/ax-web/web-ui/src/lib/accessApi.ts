export async function accessJson<T>(url: string, init?: RequestInit): Promise<T> {
  let csrf: string | null = null;
  if (init?.method && init.method !== 'GET') {
    const session = await fetch('/api/access/session', { cache: 'no-store', signal: init.signal });
    if (!session.ok) throw new Error('A local owner session is required to change access.');
    csrf = (await session.json()).csrf;
  }
  const response = await fetch(url, {
    ...init, cache: 'no-store',
    headers: { 'Content-Type': 'application/json', ...(csrf ? { 'x-ax-access-csrf': csrf } : {}), ...init?.headers },
  });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error ?? `HTTP ${response.status}`);
  return body as T;
}

