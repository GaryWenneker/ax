import { expect, test, type Page } from '@playwright/test';

import { emptyQualitySnapshot } from '../src/lib/mcpQuality';
import {
  classifyTraceBody,
  entryHasTextPayload,
  entryTextPayloads,
  parseTraceEntry,
  primaryTextPayload,
} from '../src/lib/mcpTrace';

const MARKER = `logging-text-e2e-${Date.now()}`;

// Serve the trace lines through page.route so the spec never touches the
// verbose log of the Command Center it runs against.
async function serveTrace(page: Page, lines: string[]): Promise<void> {
  let sent = false;
  await page.route('**/api/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/usage/mcp-trace/events') {
      const body = sent ? ': heartbeat\n\n' : `event: batch\ndata: ${JSON.stringify(lines)}\n\n`;
      sent = true;
      return route.fulfill({ contentType: 'text/event-stream', body });
    }
    const json =
      path === '/api/usage/mcp-quality'
        ? { ...emptyQualitySnapshot(), projectLabel: 'Logging text', verbosePresent: true, verboseEnabled: true }
        : path === '/api/usage/mcp-trace/path'
          ? { projectLabel: 'Logging text', projectRoot: '/fixtures/logging', logDay: '2026-07-26', logExists: true }
          : path === '/api/ship/config'
            ? { config: { ship: {}, ui: {} }, sonar: {} }
            : path === '/api/stats'
              ? { project_name: 'Logging text', node_count: 0, edge_count: 0, file_count: 0, languages: [], db_size_bytes: 0 }
              : {};
    return route.fulfill({ json });
  });
}

test.describe('logging text + domain kinds', () => {
  test('TOOL column labels domain and cli cmd lines', () => {
    expect(parseTraceEntry('[ax] memory remember ok id=abc').tool).toBe('memory');
    expect(parseTraceEntry('[ax] cli cmd=explore ok').tool).toBe('cli:explore');
    expect(parseTraceEntry('[ax] workspace index ok files=10 tool=workspace').tool).toBe(
      'workspace',
    );
    expect(
      parseTraceEntry(
        '2026-07-26T12:00:00.000Z [ax-mcp] inbound tool=ax_preflight args={}',
      ).tool,
    ).toBe('ax_preflight');
  });

  test('Logging UI shows TOOL=memory for domain line (not dash)', async ({ page }) => {
    const marker = `mem-tool-${Date.now()}`;
    await serveTrace(page, [`2026-07-26T12:00:00.000Z [ax] memory remember ok id=${marker} tool=memory`]);

    await page.goto('/logging');
    const rows = page.locator('.mcp-trace-row');
    await expect(rows).toHaveCount(1, { timeout: 20_000 });
    await expect(rows.first().locator('.page-item-title')).toContainText(/memory/i);
  });

  test('classify domain prefixes memory / policy / cli', () => {
    expect(classifyTraceBody('[ax] memory remember ok id=abc').kind).toBe('memory');
    expect(classifyTraceBody('[ax] policy index ok rules=1 skills=2').kind).toBe('policy');
    expect(classifyTraceBody('[ax] cli cmd=explore ok').kind).toBe('cli');
    expect(classifyTraceBody('[ax] ship start mode=evaluate').kind).toBe('ship');
    expect(classifyTraceBody('[ax] workspace index ok files=10 duration_ms=1').kind).toBe(
      'workspace',
    );
  });

  test('entryTextPayloads finds prompt and prefers it as primary', () => {
    const entry = parseTraceEntry(
      `2026-07-26T12:00:00.000Z [ax-mcp] inbound tool=ax_preflight args={"prompt":"Ship the logging text plan now","files":["a.rs"]}`,
    );
    expect(entryHasTextPayload(entry)).toBe(true);
    const payloads = entryTextPayloads(entry);
    expect(payloads.some((p) => p.leaf === 'prompt')).toBe(true);
    const primary = primaryTextPayload(entry);
    expect(primary?.leaf).toBe('prompt');
    expect(primary?.value).toContain('logging text plan');
  });

  test('Logging UI shows text badge and Prompt / text hero', async ({ page }) => {
    await serveTrace(page, [
      `2026-07-26T12:00:00.000Z [ax-mcp] inbound tool=ax_preflight args={"prompt":"${MARKER} verify inspector"}`,
    ]);

    await page.goto('/logging');

    const row = page.locator('.mcp-trace-row', { hasText: MARKER }).first();
    await expect(row).toBeVisible({ timeout: 20_000 });
    await expect(row.locator('.mcp-trace-badge--text-prompt, .mcp-trace-badge--text')).toContainText(
      /prompt/i,
    );

    await row.click();
    await expect(page.locator('.mcp-inspect-section--text-hero')).toBeVisible();
    await expect(page.locator('.mcp-inspect-text-hero')).toContainText(MARKER);
  });
});
