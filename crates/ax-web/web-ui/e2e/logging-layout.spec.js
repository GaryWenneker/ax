import { test, expect } from '@playwright/test';
import { emptyQualitySnapshot } from '../src/lib/mcpQuality';
const streamFixture = process.env.AX_SSE_FIXTURE_URL;
const lines = [
    '2026-10-12T20:00:00Z [ax-mcp] inbound tool=ax_search args={"query":"Restore readable logging events with useful details"}',
    '2026-10-12T20:00:01Z [ax-mcp] outbound tool=ax_search result={"count":3}',
    '2026-10-12T20:00:02Z [ax-mcp] internal tool=ax_status index refreshed',
];
test.beforeEach(async ({ page }) => {
    if (streamFixture) {
        const configured = await page.request.post(`${streamFixture}/configure`, { data: { lines } });
        expect(configured.ok()).toBe(true);
    }
    let sent = false;
    await page.route('**/api/**', route => {
        const path = new URL(route.request().url()).pathname;
        if (path === '/api/usage/mcp-trace/events') {
            if (streamFixture)
                return route.continue();
            const body = sent ? ': heartbeat\n\n' : `event: batch\ndata: ${JSON.stringify(lines)}\n\n`;
            sent = true;
            return route.fulfill({ contentType: 'text/event-stream', body });
        }
        const json = path === '/api/usage/mcp-quality' ? { ...emptyQualitySnapshot(), projectLabel: 'Logging layout', verbosePresent: true, verboseEnabled: true } : path === '/api/usage/mcp-trace/path' ? { projectLabel: 'Logging layout', projectRoot: '/fixtures/logging', logDay: '2026-10-12', logExists: true } : path === '/api/ship/config' ? { config: { ship: {}, ui: {} }, sonar: {} } : path === '/api/stats' ? { project_name: 'Logging layout', node_count: 0, edge_count: 0, file_count: 0, languages: [], db_size_bytes: 0 } : {};
        return route.fulfill({ json });
    });
});
test('logging retains stacked left-aligned event details at every panel width', async ({ page }) => {
    await page.goto('/logging');
    const rows = page.locator('.mcp-trace-row');
    await expect(rows).toHaveCount(3);
    await expect(page.locator('.mcp-trace-row--outbound .page-item-title')).toContainText('ax_search');
    for (const width of [2048, 1366, 900, 393]) {
        await page.setViewportSize({ width, height: 900 });
        const row = rows.first();
        await expect(row).toBeVisible();
        const geometry = await row.evaluate(e => {
            const body = e.querySelector('.page-item-body');
            const title = body.querySelector('.page-item-title-line').getBoundingClientRect();
            const sub = body.querySelector('.page-item-sub').getBoundingClientRect();
            return { left: Math.abs(title.left - sub.left), gap: sub.top - title.bottom, overflow: e.scrollWidth > e.clientWidth + 1 };
        });
        expect(geometry.left).toBeLessThanOrEqual(1);
        expect(geometry.gap).toBeLessThanOrEqual(3);
        expect(geometry.gap).toBeGreaterThanOrEqual(0);
        expect(geometry.overflow).toBe(false);
        if (process.env.AX_COLUMN_SCREENSHOTS && test.info().project.name === 'desktop-chrome' && (width === 1366 || width === 393))
            await page.screenshot({ path: `../../../site/public/screenshots/cc-logging-columns${width === 393 ? '-mobile' : ''}.png` });
    }
    await page.setViewportSize({ width: 2048, height: 900 });
    await rows.first().locator('.page-item-title').click();
    await expect(page.locator('.mcp-blade-step').first()).toBeVisible();
    await expect(rows.first()).toHaveClass(/page-item--selected/);
});
test('logging kind filters still show matching events', async ({ page }) => {
    await page.goto('/logging');
    await expect(page.locator('.mcp-trace-row')).toHaveCount(3);
    await page.locator('.mcp-trace-row--inbound button.mcp-kind-badge').click();
    await expect(page.locator('.mcp-trace-row')).toHaveCount(1);
    await expect(page.locator('.mcp-trace-row').first()).toHaveClass(/mcp-trace-row--inbound/);
});
test('logging keyboard navigation follows the sorted visible rows', async ({ page }) => {
    await page.setViewportSize({ width: 2048, height: 900 });
    await page.goto('/logging');
    await expect(page.locator('.mcp-trace-row')).toHaveCount(3);
    await page.locator('.mcp-trace-calm .column-header').getByRole('button', { name: 'Sort Time', exact: true }).click();
    const scroller = page.locator('.mcp-trace-scroller');
    await scroller.focus();
    await scroller.press('Home');
    await expect(page.locator('.mcp-trace-row').first()).toHaveAttribute('aria-selected', 'true');
    await scroller.press('ArrowDown');
    await expect(page.locator('.mcp-trace-row').nth(1)).toHaveAttribute('aria-selected', 'true');
});
test('live log arrivals obey active column sort and filters', async ({ page }, info) => {
    const streamId = 'live-' + info.project.name;
    let deliveries = 0;
    await page.route('**/api/usage/mcp-trace/events', route => {
        if (streamFixture)
            return route.continue();
        const body = deliveries++ === 0 ? `event: batch\ndata: ${JSON.stringify(lines)}\n\n` : deliveries === 2 ? 'event: line\ndata: 2026-10-12T19:59:00Z [ax-mcp] inbound tool=ax_search args={"query":"Earlier arriving event"}\n\n' : ': heartbeat\n\n';
        return route.fulfill({ contentType: 'text/event-stream', body });
    });
    await page.setViewportSize({ width: 2048, height: 900 });
    await page.goto('/logging');
    await expect(page.locator('.mcp-trace-row')).toHaveCount(3);
    const header = page.locator('.mcp-trace-calm .column-header');
    await header.getByRole('button', { name: 'Sort Time', exact: true }).click();
    await header.locator('summary[aria-label="Filter Context"]').click();
    await header.getByRole('textbox', { name: 'Filter Context', exact: true }).fill('ax_search');
    await expect(page.locator('.mcp-trace-row')).toHaveCount(2);
    if (streamFixture) {
        const emitted = await page.request.get(`${streamFixture}/emit?id=${streamId}&line=${encodeURIComponent('2026-10-12T19:59:00Z [ax-mcp] inbound tool=ax_search args={"query":"Earlier arriving event"}')}`);
        expect(emitted.ok()).toBe(true);
    }
    await expect(page.locator('.mcp-trace-row')).toHaveCount(3);
    await expect(page.locator('.mcp-trace-row').first()).toContainText('Earlier arriving event');
});
test('logging kind chips fit their compact context labels', async ({ page }) => {
    await page.setViewportSize({ width: 1366, height: 900 });
    await page.goto('/logging');
    await expect(page.locator('.mcp-trace-row')).toHaveCount(3);
    const widths = await page.locator('.mcp-trace-row .mcp-kind-badge').evaluateAll(es => es.map(e => e.getBoundingClientRect().width));
    expect(widths).toHaveLength(3);
    expect(Math.max(...widths)).toBeLessThanOrEqual(64);
});
test('logging heading filter keyboard does not open an event', async ({ page }) => {
    await page.setViewportSize({ width: 1366, height: 900 });
    await page.goto('/logging');
    await expect(page.locator('.mcp-trace-row')).toHaveCount(3);
    const filter = page.locator('.mcp-trace-calm .column-filter').nth(1);
    const summary = filter.locator('summary');
    await summary.focus();
    await summary.press('Enter');
    await expect(filter).toHaveAttribute('open', '');
    await expect(page.locator('.mcp-trace-row.page-item--selected')).toHaveCount(0);
    const input = filter.getByRole('textbox', { name: 'Filter Time', exact: true });
    await input.focus();
    await input.press('Escape');
    await expect(filter).not.toHaveAttribute('open', '');
    await expect(summary).toBeFocused();
});
test('logging has one compact page heading and no duplicate summary bars', async ({ page }) => {
    await page.setViewportSize({ width: 1366, height: 900 });
    await page.goto('/logging');
    await expect(page.getByRole('heading', { name: 'Logging', exact: true })).toBeVisible();
    await expect(page.locator('.mcp-trace-project-banner')).toHaveCount(0);
    await expect(page.locator('.mcp-q-strip')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Open MCP quality', exact: true })).toBeVisible();
    const filter = page.getByRole('search', { name: 'Filter MCP log' });
    expect((await filter.boundingBox()).height).toBeLessThan(150);
});
test('logging markers have a separate gutter and no unrelated dotted connector', async ({ page }) => {
    await page.goto('/logging');
    await expect(page.locator('.mcp-trace-row')).toHaveCount(3);
    for (const width of [2048, 1366, 900, 393]) {
        await page.setViewportSize({ width, height: 900 });
        const geometry = await page.locator('.mcp-trace-row').first().evaluate(row => {
            const dot = row.querySelector('.page-item-node').getBoundingClientRect();
            const title = row.querySelector('.page-item-title').getBoundingClientRect();
            return { gap: title.left - dot.right, dotWidth: dot.width, connector: getComputedStyle(row, '::before').display, overflow: row.scrollWidth > row.clientWidth + 1, pageOverflow: document.documentElement.scrollWidth > innerWidth + 1 };
        });
        expect(geometry.gap, `marker gutter at ${width}px`).toBeGreaterThanOrEqual(12);
        expect(geometry.dotWidth).toBeLessThanOrEqual(12);
        expect(geometry.connector).toBe('none');
        expect(geometry.overflow).toBe(false);
        expect(geometry.pageOverflow).toBe(false);
    }
});
test('real domain lines show readable actions and preserve raw-field search', async ({ page }) => {
    const domainLines = [
        '2026-10-12T20:00:00Z [ax] 🪓 workspace sync ok files=19 duration_ms=162 tool=workspace',
        '2026-10-12T20:00:01Z [ax] 🪓 memory capture-git ok scanned=20 captured=2 tool=memory',
        '2026-10-12T20:00:02Z [ax] 🪓 ship ok mode=evaluate passed=1 tool=ship',
    ];
    if (streamFixture) {
        const response = await page.request.post(`${streamFixture}/configure`, { data: { lines: domainLines } });
        expect(response.ok()).toBe(true);
    }
    else
        await page.route('**/api/usage/mcp-trace/events', route => route.fulfill({ contentType: 'text/event-stream', body: `event: batch\ndata: ${JSON.stringify(domainLines)}\n\n` }));
    await page.goto('/logging');
    await expect(page.locator('.mcp-trace-row')).toHaveCount(3);
    const workspace = page.locator('.mcp-trace-row--workspace');
    await expect(workspace.locator('.page-item-title')).toHaveText('Workspace sync completed');
    await expect(workspace.locator('.page-item-sub')).toHaveText('19 files · 162 ms');
    await expect(page.locator('.mcp-trace-row--memory .page-item-title')).toHaveText('Git history captured');
    await expect(page.locator('.mcp-trace-row--ship .page-item-title')).toHaveText('Quality check completed');
    await expect(workspace.locator('.item-column-context button')).toHaveCount(1);
    await expect(workspace).not.toContainText('tool=workspace');
    await expect(workspace).not.toContainText('🪓');
    const columns = page.locator('.mcp-trace-calm .column-header');
    await page.setViewportSize({ width: 1366, height: 900 });
    await columns.locator('summary[aria-label="Filter Event"]').click();
    await columns.getByRole('textbox', { name: 'Filter Event', exact: true }).fill('duration_ms=162');
    await expect(page.locator('.mcp-trace-row')).toHaveCount(1);
    await expect(workspace).toBeVisible();
    await workspace.locator('.page-item-title').click();
    await expect(page.locator('.mcp-blade-step').first()).toBeVisible();
    await page.locator('.mcp-inspect-fold--raw summary').click();
    await expect(page.locator('.mcp-inspect-raw')).toHaveText(domainLines[0]);
    await page.getByRole('complementary', { name: 'Log event details' }).getByRole('button', { name: 'Close', exact: true }).click();
    await page.setViewportSize({ width: 393, height: 900 });
    const mobile = await workspace.evaluate(row => {
        const time = row.querySelector('.page-item-meta').getBoundingClientRect(), context = row.querySelector('.item-column-context').getBoundingClientRect();
        return { height: row.getBoundingClientRect().height, metaAlignment: Math.abs((time.top + time.height / 2) - (context.top + context.height / 2)) };
    });
    expect(mobile.metaAlignment).toBeLessThanOrEqual(2);
    expect(mobile.height).toBeLessThanOrEqual(72);
});
test('inspector steps have a marker gutter and label the selected tool', async ({ page }) => {
    const mixed = [...lines, '2026-10-12T20:00:03Z [ax] 🪓 workspace sync ok files=19 duration_ms=162 tool=workspace'];
    if (streamFixture) {
        const response = await page.request.post(`${streamFixture}/configure`, { data: { lines: mixed } });
        expect(response.ok()).toBe(true);
    }
    else
        await page.route('**/api/usage/mcp-trace/events', route => route.fulfill({ contentType: 'text/event-stream', body: `event: batch\ndata: ${JSON.stringify(mixed)}\n\n` }));
    await page.goto('/logging');
    await expect(page.locator('.mcp-trace-row')).toHaveCount(4);
    await page.locator('.mcp-trace-row--workspace .page-item-title').click();
    const blade = page.getByRole('complementary', { name: 'Log event details' });
    await expect(blade.locator('.mcp-blade-step')).toHaveCount(4);
    const geometry = await blade.locator('.mcp-blade-step').first().evaluate(row => {
        const node = row.querySelector('.page-item-node').getBoundingClientRect(), title = row.querySelector('.page-item-title').getBoundingClientRect();
        return title.left - node.right;
    });
    expect(geometry).toBeGreaterThanOrEqual(12);
    await expect(blade.locator('.detail-header .detail-title')).toHaveText('workspace');
    await expect(blade.getByText('Tool', { exact: true }).locator('..')).toContainText('workspace');
    await expect(blade.locator('.mcp-blade-step').last().locator('.page-item-title')).toHaveText('Workspace sync completed');
});
//# sourceMappingURL=logging-layout.spec.js.map