import { createServer, type ServerResponse } from 'node:http';
import type { AddressInfo } from 'node:net';

import { expect, type Locator, type Page, type Route } from '@playwright/test';

import { emptyQualitySnapshot } from '../src/lib/mcpQuality';

/** Answers one request itself, for endpoints that need more than one canned body. */
export type Handler = (route: Route, url: URL) => Promise<void> | void;

/** A canned JSON body, or a handler. */
export type Reply = Handler | object | string | number | boolean | null;

/** /api/stats as the status bar and Stats page read it. */
export function statsStub(overrides: Record<string, unknown> = {}) {
  return {
    readonly: false,
    project_name: 'Fixture',
    node_count: 0,
    edge_count: 0,
    file_count: 0,
    languages: [],
    db_size_bytes: 0,
    ...overrides,
  };
}

/** /api/ship/config with every section the Settings page reads. */
export function shipConfigStub(overrides: { ship?: object; ui?: object } = {}) {
  return {
    config: {
      ship: { target_branch: 'main', web_port: 7070, ...overrides.ship },
      quality_gate: { steps: [], tests: { runner: 'cargo test' } },
      remote: { provider: 'github' },
      sonar: {
        enabled: false,
        host: 'http://127.0.0.1:9000',
        project_key: '',
        token_env: 'SONAR_TOKEN',
        scanner_path: 'sonar-scanner',
        container_runtime: 'podman',
      },
      ui: { ...overrides.ui },
      reviewers: {},
    },
    sonar: { runtimes: [], preferred: null, container: null, reachable: false, host: '' },
  };
}

/** One rule or skill row as /api/policy/rules and /api/policy/skills list it. */
export function policyRow(overrides: Record<string, unknown> = {}) {
  return {
    id: 'responsive-row',
    name: 'responsive-row',
    level: 'INFO',
    scope: 'project',
    body: 'A readable description of a rule or skill.',
    enabled: true,
    status: 'approved',
    alwaysApply: true,
    priority: 1,
    tags: ['graph', 'workflow'],
    globs: [],
    triggers: [],
    origin: 'project',
    sourcePath: 'rules/responsive-row.mdc',
    effectiveStorage: 'files',
    ...overrides,
  };
}

/** One memory row as /api/memory lists it. */
export function memoryRow(overrides: Record<string, unknown> = {}) {
  return {
    id: 'responsive-memory',
    kind: 'note',
    title: 'Responsive memory',
    body: 'Keep useful context visible.',
    tags: ['graph', 'workflow'],
    files: [],
    confidence: 1,
    source: 'manual',
    enabled: true,
    created_at: 1791800000000,
    updated_at: 1791800000000,
    ...overrides,
  };
}

/** Endpoints every page shell calls. */
export function baseApi(): Record<string, Reply> {
  return {
    '/api/web-auth/status': { enabled: false },
    '/api/stats': statsStub(),
    '/api/usage/mcp-quality': { ...emptyQualitySnapshot(), projectLabel: 'Fixture' },
    '/api/ship/config': shipConfigStub(),
    '/api/share/status': { readonly: false },
  };
}

/** The shared stub for an endpoint every page shell calls, or undefined. Specs with
 *  their own route handler use it as the fallback before answering 404. */
export function baseReply(path: string): unknown {
  const reply = baseApi()[path];
  return typeof reply === 'function' ? undefined : reply;
}

/**
 * Serve the Command Center API from fixtures. `overrides` maps a path (without a
 * trailing slash) to a reply and replaces the base reply for that path. Any other
 * endpoint answers 404, so a page that needs an endpoint the test did not provide
 * shows its error state instead of rendering an empty object as data.
 */
export async function mockApi(page: Page, overrides: Record<string, Reply> = {}): Promise<void> {
  const table: Record<string, Reply> = { ...baseApi(), ...overrides };
  await page.route('**/api/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.length > 1 ? url.pathname.replace(/\/$/, '') : url.pathname;
    const reply = table[path];
    if (reply === undefined) {
      return route.fulfill({ status: 404, json: { error: `Fixture endpoint not provided: ${url.pathname}` } });
    }
    if (typeof reply === 'function') return reply(route, url);
    return route.fulfill({ json: reply });
  });
}

/** A Server-Sent Events body as /api/graph/stream sends it. */
export function graphStreamBody(nodes: object[], edges: object[]): string {
  const events = [
    { type: 'meta', total_nodes: nodes.length, truncated: false, node_count: nodes.length, edge_count: edges.length },
    { type: 'nodes', nodes },
    { type: 'edges', edges },
    { type: 'done' },
  ];
  return events.map((e) => `data: ${JSON.stringify(e)}\n\n`).join('');
}

export type TraceStreamOptions = {
  /** Fields of the `project` event, over the fixture defaults. */
  project?: Record<string, unknown>;
};

/** The frames the server sends when an /api/usage/mcp-trace/events connection opens. */
export function traceSnapshotBody(lines: string[], opts: TraceStreamOptions = {}): string {
  const project = {
    path: '/fixtures/.ax/logs/mcp-verbose-2026-10-12.log',
    projectRoot: '/fixtures',
    logDay: '2026-10-12',
    logExists: true,
    verboseMcp: true,
    ...opts.project,
  };
  return (
    'event: reset\ndata: project /fixtures\n\n' +
    `event: project\ndata: ${JSON.stringify(project)}\n\n` +
    `event: batch\ndata: ${JSON.stringify(lines)}\n\n` +
    'event: ready\ndata: following\n\n'
  );
}

export type TraceStream = {
  /** Append lines to the log and send them as `line` events on every open connection. */
  send(lines: string[]): void;
};

/**
 * Serve /api/usage/mcp-trace/events the way the server does: every connection opens
 * with `reset`, `project`, the `batch` snapshot of the log so far and `ready`, then
 * stays open for live lines. A fulfilled Playwright route would end the stream at
 * once, so the request is sent on to a small streaming server in the test process.
 */
export async function routeTraceStream(page: Page, lines: string[], opts: TraceStreamOptions = {}): Promise<TraceStream> {
  const log = [...lines];
  const open = new Set<ServerResponse>();
  const server = createServer((req, res) => {
    res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
    res.write(traceSnapshotBody(log, opts));
    open.add(res);
    req.on('close', () => open.delete(res));
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address() as AddressInfo;
  page.once('close', () => {
    server.closeAllConnections();
    server.close();
  });
  await page.route('**/api/usage/mcp-trace/events', (route) =>
    route.continue({ url: `http://127.0.0.1:${port}/api/usage/mcp-trace/events` }),
  );
  return {
    send(more) {
      log.push(...more);
      const frames = more.map((line) => `event: line\ndata: ${line}\n\n`).join('');
      for (const res of open) res.write(frames);
    },
  };
}

/** Where each graph node is drawn once the layout has settled, uncovered by panels. */
export async function settledGraphNodes(page: Page, scope: Locator = page.locator('body')) {
  const wrap = scope.locator('.graph-canvas-wrap');
  await expect(wrap).toHaveAttribute('data-settled', 'true', { timeout: 20_000 });
  return wrap.evaluate((el) => {
    const probe = el as HTMLElement & { axGraphNodes?: () => { key: string; x: number; y: number }[] };
    const canvas = el.querySelector('canvas');
    return (probe.axGraphNodes?.() ?? []).filter((n) => document.elementFromPoint(n.x, n.y) === canvas);
  });
}
