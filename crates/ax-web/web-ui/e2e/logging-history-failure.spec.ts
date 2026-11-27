import { expect, test, type Page } from '@playwright/test';

import { emptyQualitySnapshot } from '../src/lib/mcpQuality';

type ChunkReply = 'html500' | 'notOk';

// Today's log is empty and the chunk endpoint for older days keeps failing.
async function serve(page: Page, reply: ChunkReply, counter: { chunks: number; healthy: boolean; streams: number }): Promise<void> {
  await page.route('**/api/**', (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname;
    if (path === '/api/usage/mcp-trace/events') {
      // Reconnect at once so the view stays live between fulfilled responses.
      const body = counter.streams++ === 0 ? 'retry: 20\nevent: batch\ndata: []\n\nevent: ready\ndata: 1\n\n' : 'retry: 20\n: heartbeat\n\n';
      return route.fulfill({ contentType: 'text/event-stream', body });
    }
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
            : {};
    return route.fulfill({ json });
  });
  await page.setViewportSize({ width: 1600, height: 900 });
}

for (const reply of ['html500', 'notOk'] as const) {
  test(`a failing history chunk (${reply}) is not retried in a loop`, async ({ page, isMobile }) => {
    test.skip(isMobile, 'desktop layout');
    const counter = { chunks: 0, healthy: false, streams: 0 };
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
