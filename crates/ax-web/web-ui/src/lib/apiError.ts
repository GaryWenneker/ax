/** A failed Command Center API call: the HTTP status and the server's message when it sent one. */
export class ApiError extends Error {
  readonly status: number;
  readonly body: unknown;

  constructor(message: string, status: number, body: unknown = null) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.body = body;
  }
}

/** The server's `error` message, with any field errors appended; null when the body has none. */
export function errorMessage(body: unknown): string | null {
  if (!body || typeof body !== 'object' || !('error' in body)) return null;
  const { error, fields } = body as { error?: unknown; fields?: unknown };
  if (typeof error !== 'string' || !error.trim()) return null;
  if (fields && typeof fields === 'object' && Object.keys(fields).length > 0) {
    const details = Object.entries(fields as Record<string, unknown>)
      .map(([field, msg]) => `${field}: ${String(msg)}`)
      .join('; ');
    return `${error} (${details})`;
  }
  return error;
}

/**
 * Throw an {@link ApiError} for a non-2xx response whose body is not read as
 * JSON data (a download or a form upload). Its message is the server's
 * error (and field errors) when the body is a JSON error, else `HTTP <status>`.
 */
export async function ensureOk(res: Response): Promise<void> {
  if (res.ok) return;
  const text = await res.text().catch(() => '');
  let data: unknown = null;
  try {
    data = text.trim() ? JSON.parse(text) : null;
  } catch {
    data = null;
  }
  throw new ApiError(errorMessage(data) ?? `HTTP ${res.status}`, res.status, data);
}
