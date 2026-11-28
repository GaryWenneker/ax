// What comes back from a remote server is checked here before the pages use
// it. A server that sends something else gets its odd entries dropped; it can
// never make a page throw.

export interface Tool {
  name: string;
  description: string;
  inputSchema: unknown;
  /** False when the server marks the tool as changing data. */
  readOnly: boolean;
  /** True when the server asks for the `ax:admin` scope to call the tool. */
  admin: boolean;
}

/** The scope a remote Ax asks for on a tool that needs an admin grant. */
export const ADMIN_SCOPE = 'ax:admin';

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Whether a tool's security schemes (top level or `_meta`) ask for the admin scope. */
function asksForAdmin(entry: Record<string, unknown>): boolean {
  const meta = isRecord(entry._meta) ? entry._meta : {};
  return [entry.securitySchemes, meta.securitySchemes].some(
    (schemes) =>
      Array.isArray(schemes) &&
      schemes.some((scheme) => isRecord(scheme) && Array.isArray(scheme.scopes) && scheme.scopes.includes(ADMIN_SCOPE)),
  );
}

export function toolsFrom(value: unknown): Tool[] {
  if (!Array.isArray(value)) return [];
  const tools: Tool[] = [];
  const seen = new Set<string>();
  for (const entry of value) {
    if (!isRecord(entry) || typeof entry.name !== 'string' || !entry.name || seen.has(entry.name)) continue;
    seen.add(entry.name);
    const annotations = isRecord(entry.annotations) ? entry.annotations : {};
    tools.push({
      name: entry.name,
      description: typeof entry.description === 'string' ? entry.description : '',
      inputSchema: entry.inputSchema,
      readOnly: annotations.readOnlyHint !== false,
      admin: asksForAdmin(entry),
    });
  }
  return tools;
}

export interface LogEntry {
  id: number;
  at: number;
  tool: string;
  transport: string;
  ok: boolean;
  durationMs: number | null;
  responseTokens: number;
}

export interface LogPage {
  entries: LogEntry[];
  /** Pass as `before` for the next older page; null on the last page. */
  nextBefore: number | null;
}

/** The page of an `ax_logs` result, or null when it has another shape. */
export function logPageFrom(value: unknown): LogPage | null {
  if (!isRecord(value) || !Array.isArray(value.entries)) return null;
  const entries: LogEntry[] = [];
  for (const entry of value.entries) {
    if (!isRecord(entry)) continue;
    const { id, at, tool, transport, ok, durationMs, responseTokens } = entry;
    if (typeof id !== 'number' || typeof at !== 'number' || typeof tool !== 'string') continue;
    entries.push({
      id,
      at,
      tool,
      transport: typeof transport === 'string' ? transport : '',
      ok: ok === true,
      durationMs: typeof durationMs === 'number' ? durationMs : null,
      responseTokens: typeof responseTokens === 'number' ? responseTokens : 0,
    });
  }
  return { entries, nextBefore: typeof value.nextBefore === 'number' ? value.nextBefore : null };
}
