import type { Handler } from '@netlify/functions';
import { jwtVerify } from 'jose';

const CORS_HEADERS = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'Content-Type, Authorization',
  'Access-Control-Allow-Methods': 'GET, OPTIONS',
};

async function verifyToken(authHeader: string | undefined, secret: string): Promise<boolean> {
  if (!authHeader?.startsWith('Bearer ')) return false;
  try {
    await jwtVerify(authHeader.slice(7), new TextEncoder().encode(secret));
    return true;
  } catch {
    return false;
  }
}

type HogRow = unknown[];

interface HogQLResponse {
  results?: HogRow[];
  columns?: string[];
  error?: string;
}

async function runHogQL(
  posthogHost: string,
  projectId: string,
  apiKey: string,
  query: string,
): Promise<{ rows: HogRow[]; error?: string }> {
  try {
    const res = await fetch(`${posthogHost}/api/projects/${projectId}/query/`, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${apiKey}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        query: { kind: 'HogQLQuery', query },
      }),
      signal: AbortSignal.timeout(18_000),
    });

    const raw = await res.text();
    if (!res.ok) {
      console.error(`PostHog HogQL HTTP ${res.status}: ${raw.slice(0, 300)}`);
      return { rows: [], error: `PostHog HTTP ${res.status}` };
    }

    let data: HogQLResponse;
    try {
      data = JSON.parse(raw) as HogQLResponse;
    } catch {
      return { rows: [], error: 'PostHog returned invalid JSON' };
    }

    if (data.error) {
      console.error(`PostHog HogQL error: ${data.error}`);
      return { rows: [], error: data.error };
    }

    return { rows: data.results ?? [] };
  } catch (err) {
    console.error('PostHog HogQL fetch error:', err);
    return { rows: [], error: String(err) };
  }
}

function asNumber(v: unknown): number {
  if (typeof v === 'number' && Number.isFinite(v)) return v;
  if (typeof v === 'string' && v.trim() !== '') {
    const n = Number(v);
    if (Number.isFinite(n)) return n;
  }
  return 0;
}

function asString(v: unknown): string {
  return typeof v === 'string' && v.trim() ? v.trim() : 'unknown';
}

function fillDays(grouped: Record<string, number>, days: number): Array<{ date: string; count: number }> {
  const result: Array<{ date: string; count: number }> = [];
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date(Date.now() - i * 86400 * 1000);
    const key = d.toISOString().slice(0, 10);
    result.push({ date: key, count: grouped[key] ?? 0 });
  }
  return result;
}

function rowsToCounts(rows: HogRow[], nameIdx = 0, countIdx = 1, limit = 10): Array<{ name: string; count: number }> {
  return rows
    .map((row) => ({ name: asString(row[nameIdx]), count: asNumber(row[countIdx]) }))
    .filter((r) => r.name !== 'unknown' || r.count > 0)
    .slice(0, limit);
}

async function buildDashboard(
  posthogHost: string,
  projectId: string,
  posthogKey: string,
  days: number,
) {
  const interval = `${days} DAY`;
  const warnings: string[] = [];

  const [
    overviewRes,
    installTimelineRes,
    indexTimelineRes,
    mcpToolsRes,
    cliCmdsRes,
    osRes,
    versionsRes,
  ] = await Promise.all([
    runHogQL(
      posthogHost,
      projectId,
      posthogKey,
      `SELECT
         countIf(event = 'install') AS installs,
         countIf(event = 'index') AS indexes,
         countIf(event = 'uninstall') AS uninstalls,
         sumIf(toIntOrZero(toString(properties.count)), event = 'usage_rollup' AND properties.kind = 'mcp_tool') AS tool_calls,
         sumIf(toIntOrZero(toString(properties.count)), event = 'usage_rollup' AND properties.kind = 'cli_command') AS cli_calls
       FROM events
       WHERE timestamp >= now() - INTERVAL ${interval}`,
    ),
    runHogQL(
      posthogHost,
      projectId,
      posthogKey,
      `SELECT toDate(timestamp) AS day, count() AS c
       FROM events
       WHERE event = 'install' AND timestamp >= now() - INTERVAL ${interval}
       GROUP BY day ORDER BY day`,
    ),
    runHogQL(
      posthogHost,
      projectId,
      posthogKey,
      `SELECT toDate(timestamp) AS day, count() AS c
       FROM events
       WHERE event = 'index' AND timestamp >= now() - INTERVAL ${interval}
       GROUP BY day ORDER BY day`,
    ),
    runHogQL(
      posthogHost,
      projectId,
      posthogKey,
      `SELECT toString(properties.name) AS name, sum(toIntOrZero(toString(properties.count))) AS total
       FROM events
       WHERE event = 'usage_rollup' AND properties.kind = 'mcp_tool' AND timestamp >= now() - INTERVAL ${interval}
       GROUP BY name ORDER BY total DESC LIMIT 10`,
    ),
    runHogQL(
      posthogHost,
      projectId,
      posthogKey,
      `SELECT toString(properties.name) AS name, sum(toIntOrZero(toString(properties.count))) AS total
       FROM events
       WHERE event = 'usage_rollup' AND properties.kind = 'cli_command' AND timestamp >= now() - INTERVAL ${interval}
       GROUP BY name ORDER BY total DESC LIMIT 10`,
    ),
    runHogQL(
      posthogHost,
      projectId,
      posthogKey,
      `SELECT toString(properties.os) AS os, count() AS c
       FROM events
       WHERE event IN ('install', 'index') AND timestamp >= now() - INTERVAL ${interval}
       GROUP BY os ORDER BY c DESC LIMIT 8`,
    ),
    runHogQL(
      posthogHost,
      projectId,
      posthogKey,
      `SELECT toString(properties.ax_version) AS version, count() AS c
       FROM events
       WHERE event IN ('install', 'index') AND timestamp >= now() - INTERVAL ${interval}
       GROUP BY version ORDER BY c DESC LIMIT 8`,
    ),
  ]);

  for (const res of [
    overviewRes,
    installTimelineRes,
    indexTimelineRes,
    mcpToolsRes,
    cliCmdsRes,
    osRes,
    versionsRes,
  ]) {
    if (res.error && !warnings.includes(res.error)) warnings.push(res.error);
  }

  const ov = overviewRes.rows[0] ?? [];
  const installGrouped: Record<string, number> = {};
  for (const row of installTimelineRes.rows) {
    const day = asString(row[0]).slice(0, 10);
    installGrouped[day] = asNumber(row[1]);
  }
  const indexGrouped: Record<string, number> = {};
  for (const row of indexTimelineRes.rows) {
    const day = asString(row[0]).slice(0, 10);
    indexGrouped[day] = asNumber(row[1]);
  }

  return {
    overview: {
      installs: asNumber(ov[0]),
      indexes: asNumber(ov[1]),
      uninstalls: asNumber(ov[2]),
      tool_calls: asNumber(ov[3]),
      cli_calls: asNumber(ov[4]),
      days,
    },
    timeline: {
      installs: fillDays(installGrouped, days),
      indexes: fillDays(indexGrouped, days),
    },
    tools: {
      mcp_tools: rowsToCounts(mcpToolsRes.rows),
      cli_commands: rowsToCounts(cliCmdsRes.rows),
    },
    os: {
      os: rowsToCounts(osRes.rows),
      versions: rowsToCounts(versionsRes.rows),
    },
    degraded: warnings.length > 0,
    warnings,
  };
}

const handler: Handler = async (event) => {
  if (event.httpMethod === 'OPTIONS') {
    return { statusCode: 204, headers: CORS_HEADERS, body: '' };
  }

  const adminSecret = process.env.ADMIN_SECRET;
  const posthogKey = process.env.POSTHOG_PERSONAL_KEY;
  const projectId = process.env.POSTHOG_PROJECT_ID;
  const posthogHost = (
    process.env.POSTHOG_API_HOST ||
    process.env.POSTHOG_HOST ||
    'https://us.posthog.com'
  ).replace(/\/$/, '');

  if (!adminSecret || !posthogKey || !projectId) {
    const missing = [
      !adminSecret && 'ADMIN_SECRET',
      !posthogKey && 'POSTHOG_PERSONAL_KEY',
      !projectId && 'POSTHOG_PROJECT_ID',
    ].filter(Boolean);
    return {
      statusCode: 503,
      headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
      body: JSON.stringify({
        error: 'Telemetry dashboard not configured',
        missing_env: missing,
        hint: 'Set POSTHOG_PERSONAL_KEY (personal API key with project read) and POSTHOG_PROJECT_ID on Netlify.',
      }),
    };
  }

  if (!(await verifyToken(event.headers.authorization, adminSecret))) {
    return {
      statusCode: 401,
      headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
      body: JSON.stringify({ error: 'Unauthorized' }),
    };
  }

  const params = event.queryStringParameters ?? {};
  const type = params.type ?? 'dashboard';
  const days = Math.min(Math.max(parseInt(params.days ?? '30', 10), 1), 90);

  try {
    if (type === 'dashboard') {
      const data = await buildDashboard(posthogHost, projectId, posthogKey, days);
      return {
        statusCode: 200,
        headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
        body: JSON.stringify(data),
      };
    }

    // Legacy single-type endpoints (still used if cached clients call them).
    const data = await buildDashboard(posthogHost, projectId, posthogKey, days);
    if (type === 'overview') {
      return {
        statusCode: 200,
        headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
        body: JSON.stringify(data.overview),
      };
    }
    if (type === 'timeline') {
      return {
        statusCode: 200,
        headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
        body: JSON.stringify(data.timeline),
      };
    }
    if (type === 'tools') {
      return {
        statusCode: 200,
        headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
        body: JSON.stringify(data.tools),
      };
    }
    if (type === 'os') {
      return {
        statusCode: 200,
        headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
        body: JSON.stringify(data.os),
      };
    }

    return {
      statusCode: 400,
      headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
      body: JSON.stringify({ error: `Unknown type: ${type}` }),
    };
  } catch (err) {
    console.error('admin-telemetry error:', err);
    return {
      statusCode: 502,
      headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
      body: JSON.stringify({ error: 'Upstream error', detail: String(err) }),
    };
  }
};

export { handler };
