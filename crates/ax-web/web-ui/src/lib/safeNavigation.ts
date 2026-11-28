const LOOPBACK_HOSTS = new Set(['localhost', '127.0.0.1', '[::1]']);

/**
 * The sign-in address to open, or null when it is not safe to navigate to.
 * HTTPS is accepted anywhere; plain HTTP only on this computer. Every other
 * scheme (javascript:, data:, …) and any malformed value is refused.
 */
export function signInTarget(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return null;
  }
  if (url.protocol === 'https:') return value;
  if (url.protocol === 'http:' && LOOPBACK_HOSTS.has(url.hostname)) return value;
  return null;
}
