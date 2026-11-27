import { expect, test } from '@playwright/test';

import { emptyQualitySnapshot } from '../src/lib/mcpQuality';
import { MCP_TRACE_MAX_ENTRIES } from '../src/lib/mcpTrace';

const line = (i: number) => {
  const ts = new Date(Date.UTC(2026, 9, 12, 8, 0, 0) + i * 1000).toISOString();
  return `${ts} [ax-mcp] inbound tool=ax_search session=axs_cap args={"query":"event ${i}"}`;
};

// A long session: the snapshot alone is larger than the buffer cap.
test('the trace view keeps a bounded buffer of the newest rows', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  test.setTimeout(90_000);
  const total = MCP_TRACE_MAX_ENTRIES + 300;
  const lines = Array.from({ length: total }, (_, i) => line(i));
  let streams = 0;
  await page.route('**/api/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/usage/mcp-trace/events') {
      const body = streams++ === 0
        ? `retry: 50\nevent: batch\ndata: ${JSON.stringify(lines)}\n\nevent: ready\ndata: 1\n\n`
        : 'retry: 50\n: heartbeat\n\n';
      return route.fulfill({ contentType: 'text/event-stream', body });
    }
    const json =
      path === '/api/usage/mcp-quality'
        ? { ...emptyQualitySnapshot(), projectLabel: 'Cap', verbosePresent: true, verboseEnabled: true }
        : path === '/api/usage/mcp-trace/path'
          ? { projectLabel: 'Cap', projectRoot: '/fixtures/cap', logDay: '2026-10-12', logExists: true }
          : path === '/api/ship/config'
            ? { config: { ship: {}, ui: {} }, sonar: {} }
            : {};
    return route.fulfill({ json });
  });
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto('/logging');
  await expect(page.locator('.mcp-trace-row[data-entry-id]').first()).toBeAttached({ timeout: 30_000 });
  await page.waitForTimeout(1_000);
  const rows = await page.locator('.mcp-trace-row[data-entry-id]').count();
  expect(rows).toBeLessThanOrEqual(MCP_TRACE_MAX_ENTRIES);
  await expect(page.locator('.mcp-trace-history-status')).toContainText(/older rows were dropped/i);
  // The newest line is still there; the oldest ones were dropped.
  const ids = await page.locator('.mcp-trace-row[data-entry-id]').evaluateAll((els) =>
    els.map((el) => el.textContent ?? '').join('\n'),
  );
  expect(ids).toContain(`event ${total - 1}`);
  expect(ids).not.toContain('event 0"');
});
