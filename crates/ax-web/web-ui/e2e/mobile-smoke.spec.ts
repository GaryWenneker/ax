import { expect, test, type Page } from '@playwright/test';

import { mockApi } from './fixtures';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SHOT_DIR = path.join(__dirname, '..', 'test-results', 'mobile-shots');

async function shot(page: Page, name: string) {
  await page.screenshot({
    path: path.join(SHOT_DIR, `${name}.png`),
    fullPage: true,
  });
}

async function waitForApiOk(page: Page) {
  const res = await page.request.get('/api/stats');
  expect(res.ok(), `GET /api/stats → ${res.status()}`).toBeTruthy();
  const body = await res.json();
  expect(typeof body.node_count).toBe('number');
}

test.describe('Command Center mobile smoke', () => {
  test.beforeEach(async ({ page }) => {
    await waitForApiOk(page);
  });

  test('stats loads data and shows shell', async ({ page }) => {
    await page.goto('/stats');
    await expect(page.locator('#main-content')).toBeVisible();
    await expect(page.locator('.hamburger')).toBeVisible();
    await expect(page.locator('.statusbar')).toBeVisible();
    // Stats page should render numeric content from the API eventually.
    await expect(page.locator('#main-content')).not.toBeEmpty();
    await shot(page, '01-stats');
  });

  test('hamburger opens nav and reaches logging', async ({ page }) => {
    await page.goto('/stats');
    await page.locator('.hamburger').click();
    await expect(page.locator('.sidebar.open')).toBeVisible();
    await page.locator('.sidebar .nav-item', { hasText: 'Logging' }).click();
    await expect(page).toHaveURL(/\/logging/);
    await expect(page.locator('.sidebar.open')).toHaveCount(0);
    await shot(page, '02-logging');
  });

  test('settings shows Sharing, Plugins, Embeddings', async ({ page }) => {
    await page.goto('/settings');
    await expect(page.getByText('Sharing', { exact: true }).first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText('Plugins', { exact: true }).first()).toBeVisible();
    await expect(page.getByText('Embeddings', { exact: true }).first()).toBeVisible();
    await expect(page.getByText('Extractor plugins').first()).toBeVisible();
    await expect(page.getByText('Memory embeddings').first()).toBeVisible();
    await shot(page, '03-settings');
  });

  test('unresolved and memory load', async ({ page }) => {
    await page.goto('/unresolved');
    await expect(page.locator('#main-content')).toBeVisible();
    await shot(page, '04-unresolved');

    await page.goto('/memory');
    await expect(page.locator('#main-content')).toBeVisible();
    await shot(page, '05-memory');
  });

  test('activity chip visible in status bar', async ({ page }) => {
    await page.goto('/stats');
    await expect(page.locator('.status-activity')).toBeVisible();
    await page.locator('.status-activity').click();
    await expect(page.locator('.status-panel--activity')).toBeVisible();
    await shot(page, '06-statusbar-activity');
  });

  test('project browser does not autofocus filter', async ({ page }) => {
    await page.goto('/stats');
    await page.locator('.status-project').click();
    await expect(page.getByText('Open ax project')).toBeVisible();
    const filter = page.locator('.project-browser-search');
    await expect(filter).toBeVisible();
    await expect(filter).not.toBeFocused();
    await shot(page, '07-project-browser');
  });

  test('logging dock button opens control sheet', async ({ page }) => {
    await page.goto('/logging');
    await expect(page.locator('.status-logging')).toBeVisible();
    await page.locator('.status-logging').click();
    await expect(page.getByRole('region', { name: /Logging/i })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Scroll to newest' })).toBeVisible();
    await shot(page, '09-logging-dock');
  });
});

test.describe('Command Center mobile logging (fixture API)', () => {
  test.skip(({ isMobile }) => !isMobile, 'phone layout');

  const lines = [
    '2026-10-12T20:00:00Z [ax-mcp] inbound tool=ax_search args={"query":"A long prompt that would push a fixed-width table past the edge of a phone screen"}',
    '2026-10-12T20:00:01Z [ax-mcp] outbound tool=ax_search result={"count":3}',
    '2026-10-12T20:00:02Z [ax] 🪓 workspace sync ok files=19 duration_ms=162 tool=workspace',
  ];

  test('logging rows fit the phone width without sideways scrolling', async ({ page }) => {
    let sent = false;
    await mockApi(page, {
      '/api/usage/mcp-trace/events': (route) => {
        const body = sent ? ': heartbeat\n\n' : `event: batch\ndata: ${JSON.stringify(lines)}\n\n`;
        sent = true;
        return route.fulfill({ contentType: 'text/event-stream', body });
      },
      '/api/usage/mcp-trace/path': { projectLabel: 'Fixture', projectRoot: '/fixtures', logDay: '2026-10-12', logExists: true },
    });
    await page.goto('/logging');
    const rows = page.locator('.mcp-trace-row');
    await expect(rows).toHaveCount(lines.length);
    const layout = await page.evaluate(() => {
      const scroller = document.querySelector('.mcp-trace-scroller') as HTMLElement;
      return {
        rows: [...document.querySelectorAll<HTMLElement>('.mcp-trace-row')].map((row) => row.scrollWidth - row.clientWidth),
        scroller: scroller.scrollWidth - scroller.clientWidth,
        page: document.documentElement.scrollWidth - window.innerWidth,
      };
    });
    for (const overflow of layout.rows) expect(overflow).toBeLessThanOrEqual(1);
    expect(layout.scroller).toBeLessThanOrEqual(1);
    expect(layout.page).toBeLessThanOrEqual(1);
    await shot(page, '08-logging-fit');
  });
});
