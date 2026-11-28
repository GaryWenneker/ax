import { expect, test } from '@playwright/test';

import { emptyQualitySnapshot } from '../src/lib/mcpQuality';
import { baseReply, routeTraceStream } from './fixtures';

const first = '2026-10-12T20:00:00Z [ax-mcp] inbound tool=ax_search session=axs_live args={"query":"First event"}';
const later = '2026-10-12T20:00:05Z [ax-mcp] inbound tool=ax_search session=axs_live args={"query":"Later event"}';

// Typed continuity fields must survive new live log lines.
test('typed continuity fields survive a new live log line', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  await page.route('**/api/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    const json =
      path === '/api/usage/mcp-quality'
        ? { ...emptyQualitySnapshot(), projectLabel: 'Continuity', verbosePresent: true, verboseEnabled: true }
        : path === '/api/usage/mcp-trace/path'
          ? { projectLabel: 'Continuity', projectRoot: '/fixtures/continuity', logDay: '2026-10-12', logExists: true }
          : path === '/api/ship/config'
            ? { config: { ship: {}, ui: {} }, sonar: {} }
            : (baseReply(path) ?? {});
    return route.fulfill({ json });
  });
  const stream = await routeTraceStream(page, [first]);
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto('/logging');
  await expect(page.locator('.mcp-trace-row')).toHaveCount(1);
  await page.getByRole('button', { name: 'Context continuity' }).click();
  const task = page.locator('.continuity-task textarea');
  await expect(task).toBeVisible();
  await task.fill('My own task for the next chat');
  const agent = page.locator('.continuity-grid').getByLabel('Target agent', { exact: true });
  await agent.fill('codex');

  stream.send([later]);
  await expect(page.locator('.mcp-trace-row')).toHaveCount(2, { timeout: 15_000 });
  await expect(task).toHaveValue('My own task for the next chat');
  await expect(agent).toHaveValue('codex');
});
