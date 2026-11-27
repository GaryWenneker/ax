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
