import { test, expect } from '@playwright/test';
const policy = { id: 'responsive-row', name: 'responsive-row', level: 'INFO', scope: 'project', body: 'A readable description of a rule or skill.', enabled: true, status: 'approved', alwaysApply: true, priority: 1, tags: ['graph', 'workflow'], globs: [], triggers: [], origin: 'project', sourcePath: 'rules/responsive-row.mdc', effectiveStorage: 'files' };
test.beforeEach(async ({ page }) => {
    await page.route('**/api/**', route => {
        const p = new URL(route.request().url()).pathname;
        const json = p === '/api/policy/rules' ? { rules: [policy] } : p === '/api/policy/skills' ? { skills: [policy] } : p === '/api/memory' ? { memories: [{ id: 'responsive-memory', kind: 'note', title: 'Responsive memory', body: 'Keep useful context visible.', tags: ['graph', 'workflow'], files: [], confidence: 1, source: 'manual', enabled: true, created_at: 1791800000000, updated_at: 1791800000000 }], total: 1 } : p === '/api/ship/config' ? { config: { ship: {}, ui: {} }, sonar: {} } : p === '/api/stats' ? { project_name: 'Responsive UI', node_count: 0, edge_count: 0, file_count: 0, languages: [], db_size_bytes: 0 } : {};
        return route.fulfill({ json });
    });
});
test('fields and keyboard buttons get a soft glow without thick outlines', async ({ page }) => {
    for (const mode of ['azure-light', 'azure-dark']) {
        await page.goto('/stats');
        await page.evaluate(m => localStorage.setItem('ax-theme', m), mode);
        await page.goto('/policy/rules');
        const input = page.getByRole('combobox', { name: 'Filter rules by labels and text' });
        await input.focus();
        await expect(input).toHaveCSS('outline-width', '0px');
        const glow = await input.evaluate(e => getComputedStyle(e.closest('.label-ac-box') ?? e).boxShadow);
        expect(glow).not.toBe('none');
        expect(glow).toMatch(/8px/);
        if (process.env.AX_DENSITY_SCREENSHOTS && test.info().project.name === 'desktop-chrome' && mode === 'azure-dark')
            await page.screenshot({ path: '../../../site/public/screenshots/cc-focus-glow.png' });
        const button = page.getByRole('button', { name: 'Edit responsive-row', exact: true });
        await button.scrollIntoViewIfNeeded();
        const before = await button.boundingBox();
        await button.focus();
        await expect(button).toHaveCSS('outline-width', '0px');
        expect(await button.evaluate(e => getComputedStyle(e).boxShadow)).not.toBe('none');
        expect(await button.boundingBox()).toEqual(before);
    }
});
test('system high-contrast focus remains visible', async ({ page }) => {
    await page.emulateMedia({ forcedColors: 'active' });
    await page.goto('/policy/rules');
    const input = page.getByRole('combobox', { name: 'Filter rules by labels and text' });
    await input.focus();
    await expect(input).toHaveCSS('outline-style', 'solid');
    await expect(input).toHaveCSS('outline-width', '1px');
});
test('Rules Skills and Memory use less space when wide and wrap when narrow', async ({ page }) => {
    for (const path of ['/policy/rules', '/policy/skills', '/memory']) {
        await page.setViewportSize({ width: 2048, height: 900 });
        await page.goto(path);
        const row = page.locator('.portal-data-row').first();
        await expect(row).toBeVisible();
        const wide = await row.boundingBox();
        expect(wide.height).toBeLessThanOrEqual(40);
        const elements = await row.locator('.portal-data-name > *').evaluateAll(es => es.map(e => e.getBoundingClientRect().top));
        expect(Math.max(...elements) - Math.min(...elements)).toBeLessThan(6);
        if (process.env.AX_DENSITY_SCREENSHOTS && test.info().project.name === 'desktop-chrome')
            await page.screenshot({ path: `../../../site/public/screenshots/cc-responsive-${path.split('/').slice(1).join('-')}.png` });
        await page.setViewportSize({ width: 393, height: 900 });
        const narrow = await row.boundingBox();
        expect(narrow.height).toBeGreaterThan(wide.height);
        const overflow = await row.evaluate(e => e.scrollWidth > e.clientWidth + 1);
        expect(overflow).toBe(false);
        await expect(row.locator('.portal-data-cell[data-label]').first()).toBeVisible();
        console.log(JSON.stringify({ path, wideHeight: wide.height, narrowHeight: narrow.height }));
        if (process.env.AX_DENSITY_SCREENSHOTS && test.info().project.name === 'desktop-chrome')
            await page.screenshot({ path: `../../../site/public/screenshots/cc-responsive-${path.split('/').slice(1).join('-')}-mobile.png` });
    }
});
test('long metadata fits constrained panels without horizontal scrolling', async ({ page }) => {
    const long = { ...policy, tags: ['very-long-metadata-'.repeat(12), 'graph', 'workflow'], body: 'A long explanation. '.repeat(25) };
    await page.route(/\/api\/policy\/rules(?:\?|$)/, route => route.fulfill({ json: { rules: [long] } }));
    for (const width of [2048, 1366, 900, 393]) {
        await page.setViewportSize({ width, height: 900 });
        await page.goto('/policy/rules');
        const row = page.locator('.portal-data-row').first();
        await expect(row).toBeVisible();
        const fits = await row.evaluate(e => e.scrollWidth <= e.clientWidth + 1);
        if (!fits)
            console.log(JSON.stringify(await row.evaluate(e => ({ width: e.clientWidth, scroll: e.scrollWidth, children: [...e.querySelectorAll('.portal-data-name,.portal-data-tags,.policy-view-tags,.page-item-badge')].map(c => ({ class: c.className, width: c.clientWidth, scroll: c.scrollWidth, left: c.getBoundingClientRect().left })) }))));
        expect(fits).toBe(true);
        await expect(row.getByRole('button', { name: 'Edit responsive-row', exact: true })).toBeVisible();
    }
});
test('column headings have room without touching the next heading', async ({ page }) => {
    for (const width of [1366, 2048]) {
        await page.setViewportSize({ width, height: 900 });
        for (const path of ['/policy/rules', '/policy/skills', '/memory']) {
            await page.goto(path);
            const header = page.locator('.portal-data-header').first();
            await expect(header).toBeVisible();
            expect(await header.evaluate(e => [...e.children].every(c => c.scrollWidth <= c.clientWidth + 1))).toBe(true);
            expect(await header.locator('.column-sort').evaluateAll(buttons => buttons.filter(b => b.scrollWidth > b.clientWidth + 1).map(b => ({ label: b.textContent, width: b.clientWidth, scroll: b.scrollWidth })))).toEqual([]);
            for (const button of await header.locator('.column-sort').all()) {
                await button.click();
                expect(await button.evaluate(b => b.scrollWidth <= b.clientWidth + 1)).toBe(true);
            }
        }
    }
});
test('Memory header actions stay within a narrow panel', async ({ page }) => {
    await page.setViewportSize({ width: 393, height: 900 });
    await page.goto('/memory');
    const hero = page.locator('.settings-hero').first();
    await expect(hero).toBeVisible();
    await expect(hero.locator('.settings-hero-actions button.btn')).toHaveCount(3);
    const accent = await page.getByRole('button', { name: 'Home', exact: true }).evaluate(e => getComputedStyle(e).color);
    await expect(page.getByRole('button', { name: 'Capture from git', exact: true })).toHaveCSS('color', accent);
    expect(await hero.evaluate(e => [...e.querySelectorAll('.settings-hero-actions button.btn')].every(b => b.getBoundingClientRect().right <= e.getBoundingClientRect().right + 1))).toBe(true);
});
//# sourceMappingURL=focus-row-density.spec.js.map