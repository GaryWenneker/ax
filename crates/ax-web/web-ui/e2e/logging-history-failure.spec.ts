import { expect, test, type Page } from '@playwright/test';

import { emptyQualitySnapshot } from '../src/lib/mcpQuality';
import { baseReply, routeTraceStream } from './fixtures';

type ChunkReply = 'html500' | 'notOk';

// Today's log is empty and the chunk endpoint for older days keeps failing.
async function serve(page: Page, reply: ChunkReply, counter: { chunks: number; healthy: boolean }): Promise<void> {
  await page.route('**/api/**', (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname;
    if (path === '/api/usage/mcp-trace/chunk') {
      counter.chunks++;
      if (counter.healthy) {
        return route.fulfill({
          json: {
            ok: true,
            day: '2026-10-11',
            hasOlder: false,
            lines: ['2026-10-11T20:00:00Z [ax-mcp] inbound tool=ax_search session=axs_old args={"query":"Older event"}'],
          },
        });
      }
      if (reply === 'html500') return route.fulfill({ status: 500, contentType: 'text/html', body: '<h1>oops</h1>' });
      return route.fulfill({ json: { ok: false, error: 'chunk read failed' } });
    }
    const json =
      path === '/api/usage/mcp-quality'
        ? { ...emptyQualitySnapshot(), projectLabel: 'History', verbosePresent: true, verboseEnabled: true }
        : path === '/api/usage/mcp-trace/path'
          ? { projectLabel: 'History', projectRoot: '/fixtures/history', logDay: '2026-10-12', logExists: true }
          : path === '/api/ship/config'
            ? { config: { ship: {}, ui: {} }, sonar: {} }
            : (baseReply(path) ?? {});
    return route.fulfill({ json });
  });
  // Today's log is empty: the stream sends an empty snapshot and stays open.
  await routeTraceStream(page, []);
  await page.setViewportSize({ width: 1600, height: 900 });
}

for (const reply of ['html500', 'notOk'] as const) {
  test(`a failing history chunk (${reply}) is not retried in a loop`, async ({ page, isMobile }) => {
    test.skip(isMobile, 'desktop layout');
    const counter = { chunks: 0, healthy: false };
    await serve(page, reply, counter);
    await page.goto('/logging');
    await expect.poll(() => counter.chunks).toBeGreaterThan(0);
    await page.waitForTimeout(1_500);
    expect(counter.chunks).toBe(1);

    const retry = page.locator('.mcp-trace-empty').getByRole('button', { name: /retry|previous day/i });
    await expect(page.locator('.mcp-trace-empty')).toContainText(/could not load earlier history/i);
    counter.healthy = true;
    await retry.click();
    await expect(page.locator('.mcp-trace-row')).toHaveCount(1);
  });
}
