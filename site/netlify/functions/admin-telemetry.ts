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

interface PostHogEvent {
  id: string;
  distinct_id: string;
  event: string;
  timestamp: string;
  properties: Record<string, unknown>;
}

interface PostHogEventsResponse {
  results: PostHogEvent[];
  next: string | null;
}

async function fetchEvents(
  baseUrl: string,
  projectId: string,
  apiKey: string,
  eventName: string,
  after: string,
  limit = 500,
): Promise<PostHogEvent[]> {
  const url = new URL(`${baseUrl}/api/projects/${projectId}/events/`);
  url.searchParams.set('event', eventName);
  url.searchParams.set('after', after);
  url.searchParams.set('limit', String(Math.min(limit, 500)));

  const res = await fetch(url.toString(), {
    headers: { Authorization: `Bearer ${apiKey}` },
    signal: AbortSignal.timeout(8000),
  });

  if (!res.ok) {
    console.error(`PostHog events fetch failed: ${res.status} for event=${eventName}`);
    return [];
  }

  const data = (await res.json()) as PostHogEventsResponse;
  return data.results ?? [];
}

function groupByDay(events: PostHogEvent[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const ev of events) {
    const day = ev.timestamp.slice(0, 10);
    counts[day] = (counts[day] ?? 0) + 1;
  }
  return counts;
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

function topN<T extends string>(items: T[], n = 10): Array<{ name: string; count: number }> {
  const counts: Record<string, number> = {};
  for (const item of items) counts[item] = (counts[item] ?? 0) + 1;
  return Object.entries(counts)
    .sort((a, b) => b[1] - a[1])
    .slice(0, n)
    .map(([name, count]) => ({ name, count }));
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
      !posthogKey && 'POSTHOG_PERSONAL_KEY',
      !projectId && 'POSTHOG_PROJECT_ID',
      !process.env.POSTHOG_KEY && 'POSTHOG_KEY',
    ].filter(Boolean);
    return {
      statusCode: 500,
      headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
      body: JSON.stringify({
        error: 'Server configuration error',
        missing_env: missing,
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
  const type = params.type ?? 'overview';
  const days = Math.min(Math.max(parseInt(params.days ?? '30', 10), 1), 90);
  const after = new Date(Date.now() - days * 86400 * 1000).toISOString();

  try {
    if (type === 'overview') {
      const [installs, indexes, rollups, uninstalls] = await Promise.all([
        fetchEvents(posthogHost, projectId, posthogKey, 'install', after),
        fetchEvents(posthogHost, projectId, posthogKey, 'index', after),
        fetchEvents(posthogHost, projectId, posthogKey, 'usage_rollup', after),
        fetchEvents(posthogHost, projectId, posthogKey, 'uninstall', after),
      ]);

      const totalToolCalls = rollups
        .filter((e) => e.properties.kind === 'mcp_tool')
        .reduce((sum, e) => sum + ((e.properties.count as number) ?? 0), 0);
      const totalCliCalls = rollups
        .filter((e) => e.properties.kind === 'cli_command')
        .reduce((sum, e) => sum + ((e.properties.count as number) ?? 0), 0);

      return {
        statusCode: 200,
        headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
        body: JSON.stringify({
          installs: installs.length,
          indexes: indexes.length,
          uninstalls: uninstalls.length,
          tool_calls: totalToolCalls,
          cli_calls: totalCliCalls,
          days,
        }),
      };
    }

    if (type === 'timeline') {
      const [installs, indexes] = await Promise.all([
        fetchEvents(posthogHost, projectId, posthogKey, 'install', after),
        fetchEvents(posthogHost, projectId, posthogKey, 'index', after),
      ]);

      return {
        statusCode: 200,
        headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
        body: JSON.stringify({
          installs: fillDays(groupByDay(installs), days),
          indexes: fillDays(groupByDay(indexes), days),
        }),
      };
    }

    if (type === 'tools') {
      const rollups = await fetchEvents(posthogHost, projectId, posthogKey, 'usage_rollup', after);

      const mcpTools = rollups
        .filter((e) => e.properties.kind === 'mcp_tool')
        .map((e) => e.properties.name as string)
        .filter(Boolean);
      const cliCmds = rollups
        .filter((e) => e.properties.kind === 'cli_command')
        .map((e) => e.properties.name as string)
        .filter(Boolean);

      return {
        statusCode: 200,
        headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
        body: JSON.stringify({
          mcp_tools: topN(mcpTools),
          cli_commands: topN(cliCmds),
        }),
      };
    }

    if (type === 'os') {
      const events = await Promise.all([
        fetchEvents(posthogHost, projectId, posthogKey, 'install', after),
        fetchEvents(posthogHost, projectId, posthogKey, 'index', after),
      ]).then(([a, b]) => [...a, ...b]);

      const osList = events.map((e) => (e.properties.os as string) ?? 'unknown').filter(Boolean);
      const archList = events.map((e) => (e.properties.arch as string) ?? 'unknown').filter(Boolean);
      const versionList = events.map((e) => (e.properties.ax_version as string) ?? 'unknown').filter(Boolean);

      return {
        statusCode: 200,
        headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
        body: JSON.stringify({
          os: topN(osList, 8),
          arch: topN(archList, 6),
          versions: topN(versionList, 8),
        }),
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
      body: JSON.stringify({ error: 'Upstream error' }),
    };
  }
};

export { handler };
