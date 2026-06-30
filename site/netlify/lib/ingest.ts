/**
 * Shared telemetry ingest logic (mirrors telemetry-worker/src/index.ts).
 * Used by Netlify Functions at getax.wenneker.io/v1/events
 */

const MAX_BODY_BYTES = 64 * 1024;
const MAX_EVENTS_PER_BATCH = 100;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const TOKEN_RE = /^[A-Za-z0-9_.:+-]+$/;
const LABEL_RE = /^[A-Za-z0-9_.:+/ @()-]+$/;

export const INFO_TEXT = `ax anonymous-telemetry ingest.

What gets collected (and what never does) is documented field-by-field:
https://github.com/GaryWenneker/ax/blob/main/docs/TELEMETRY.md

Disable any time: ax telemetry off  |  AX_TELEMETRY=0  |  DO_NOT_TRACK=1
`;

type JsonObject = Record<string, unknown>;
type Sanitize = (v: unknown) => unknown;

const oneOf =
  (allowed: readonly string[]): Sanitize =>
  (v) =>
    typeof v === 'string' && allowed.includes(v) ? v : undefined;

const matching =
  (re: RegExp, maxLen: number): Sanitize =>
  (v) =>
    typeof v === 'string' && v.length > 0 && v.length <= maxLen && re.test(v) ? v : undefined;

const token = (maxLen: number): Sanitize => matching(TOKEN_RE, maxLen);
const label = (maxLen: number): Sanitize => matching(LABEL_RE, maxLen);

const tokenArray =
  (maxItems: number, maxLen: number): Sanitize =>
  (v) =>
    Array.isArray(v) &&
    v.length <= maxItems &&
    v.every((s) => typeof s === 'string' && s.length > 0 && s.length <= maxLen && TOKEN_RE.test(s))
      ? v
      : undefined;

const nonNegInt =
  (max: number): Sanitize =>
  (v) =>
    typeof v === 'number' && Number.isInteger(v) && v >= 0 && v <= max ? v : undefined;

const EVENTS: Record<string, { required: readonly string[]; props: Record<string, Sanitize> }> = {
  install: {
    required: ['scope', 'kind'],
    props: {
      targets: tokenArray(12, 24),
      scope: oneOf(['local', 'global']),
      kind: oneOf(['fresh', 'upgrade', 'reinstall']),
      sqlite_backend: oneOf(['native', 'wasm']),
    },
  },
  index: {
    required: [],
    props: {
      languages: tokenArray(32, 24),
      file_count_bucket: oneOf(['<100', '100-1k', '1k-10k', '10k+']),
      duration_bucket: oneOf(['<10s', '10-60s', '1-5m', '5m+']),
      sqlite_backend: oneOf(['native', 'wasm']),
    },
  },
  usage_rollup: {
    required: ['kind', 'name', 'count'],
    props: {
      kind: oneOf(['mcp_tool', 'cli_command']),
      name: token(64),
      count: nonNegInt(1_000_000),
      error_count: nonNegInt(1_000_000),
      client_name: label(64),
      client_version: label(32),
    },
  },
  uninstall: {
    required: [],
    props: { targets: tokenArray(12, 24) },
  },
};

const ENVELOPE_PROPS: Record<string, Sanitize> = {
  ax_version: token(32),
  os: token(16),
  arch: token(16),
  node_major: nonNegInt(99),
  ci: (v) => (typeof v === 'boolean' ? v : undefined),
  schema_version: nonNegInt(99),
};

interface PostHogEvent {
  event: string;
  distinct_id: string;
  timestamp?: string;
  properties: JsonObject;
}

function clampTimestamp(v: unknown): string | undefined {
  if (typeof v !== 'string') return undefined;
  const t = Date.parse(v);
  if (!Number.isFinite(t)) return undefined;
  const now = Date.now();
  if (t > now + 10 * 60_000 || t < now - 30 * 86_400_000) return undefined;
  return new Date(t).toISOString();
}

function sanitizeEvent(raw: unknown, machineId: string, common: JsonObject): PostHogEvent | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const e = raw as JsonObject;
  if (typeof e.event !== 'string') return null;
  const spec = EVENTS[e.event];
  if (!spec) return null;

  const rawProps = (typeof e.props === 'object' && e.props !== null ? e.props : {}) as JsonObject;
  const props: JsonObject = {};
  for (const [key, sanitize] of Object.entries(spec.props)) {
    const val = sanitize(rawProps[key]);
    if (val !== undefined) props[key] = val;
  }
  for (const req of spec.required) {
    if (!(req in props)) return null;
  }

  const out: PostHogEvent = {
    event: e.event,
    distinct_id: machineId,
    properties: {
      ...props,
      ...common,
      $process_person_profile: false,
      $geoip_disable: true,
      $lib: 'ax-telemetry-netlify',
    },
  };
  const ts = clampTimestamp(e.ts);
  if (ts !== undefined) out.timestamp = ts;
  return out;
}

async function forwardToPostHog(batch: PostHogEvent[]): Promise<boolean> {
  const key = process.env.POSTHOG_KEY;
  const host = (
    process.env.POSTHOG_INGEST_HOST ||
    process.env.POSTHOG_HOST ||
    'https://us.i.posthog.com'
  ).replace(/\/$/, '');
  if (!key) {
    console.error('POSTHOG_KEY not configured — events dropped');
    return false;
  }
  try {
    const res = await fetch(`${host}/batch/`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ api_key: key, batch }),
      signal: AbortSignal.timeout(5000),
    });
    if (!res.ok) {
      console.error(JSON.stringify({ msg: 'posthog forward failed', status: res.status }));
      return false;
    }
    return true;
  } catch (err) {
    console.error(JSON.stringify({ msg: 'posthog forward error', err: String(err) }));
    return false;
  }
}

export interface TelemetryResponse {
  statusCode: number;
  body?: string;
  headers?: Record<string, string>;
}

/** Handle telemetry ingest for Netlify Functions (path /v1/events). */
export async function handleTelemetryIngest(
  method: string,
  path: string,
  bodyRaw: string | null,
  contentLengthHeader: string | null,
): Promise<TelemetryResponse> {
  if (method === 'GET' && path === '/telemetry') {
    return {
      statusCode: 200,
      body: INFO_TEXT,
      headers: { 'content-type': 'text/plain; charset=utf-8' },
    };
  }

  if (path !== '/v1/events') {
    return { statusCode: 404, body: 'not found\n' };
  }
  if (method !== 'POST') {
    return {
      statusCode: 405,
      body: 'method not allowed\n',
      headers: { allow: 'POST' },
    };
  }

  const contentLength = Number(contentLengthHeader);
  if (!Number.isFinite(contentLength) || contentLength <= 0) {
    return { statusCode: 411, body: 'length required\n' };
  }
  if (contentLength > MAX_BODY_BYTES) {
    return { statusCode: 413, body: 'payload too large\n' };
  }

  if (!bodyRaw) {
    return { statusCode: 400, body: 'bad request\n' };
  }
  if (bodyRaw.length > MAX_BODY_BYTES) {
    return { statusCode: 413, body: 'payload too large\n' };
  }

  let body: JsonObject;
  try {
    const parsed: unknown = JSON.parse(bodyRaw);
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      return { statusCode: 400, body: 'bad request\n' };
    }
    body = parsed as JsonObject;
  } catch {
    return { statusCode: 400, body: 'bad request\n' };
  }

  const machineId = body.machine_id;
  if (typeof machineId !== 'string' || !UUID_RE.test(machineId)) {
    return { statusCode: 400, body: 'bad request\n' };
  }

  const common: JsonObject = {};
  for (const [key, sanitize] of Object.entries(ENVELOPE_PROPS)) {
    const val = sanitize(body[key]);
    if (val !== undefined) common[key] = val;
  }

  const rawEvents = Array.isArray(body.events) ? body.events.slice(0, MAX_EVENTS_PER_BATCH) : [];
  const batch: PostHogEvent[] = [];
  for (const raw of rawEvents) {
    const sanitized = sanitizeEvent(raw, machineId, common);
    if (sanitized) batch.push(sanitized);
  }

  if (batch.length > 0) {
    const ok = await forwardToPostHog(batch);
    if (!ok) {
      return {
        statusCode: 503,
        body: 'telemetry backend unavailable\n',
        headers: { 'content-type': 'text/plain; charset=utf-8' },
      };
    }
  }

  return { statusCode: 204 };
}
