import { test, expect } from '@playwright/test';
test.beforeEach(async ({ page }) => {
    await page.route('**/api/**', route => {
        const p = new URL(route.request().url()).pathname;
        return route.fulfill({ json: p === '/api/ship/config' ? { config: { ship: {}, ui: {} }, sonar: {} } : p === '/api/stats' ? { project_name: 'Brand test', node_count: 0, edge_count: 0, file_count: 0, languages: [], db_size_bytes: 0 } : {} });
    });
});
test('header matches homepage typography and logo links home', async ({ page }) => {
    await page.goto('/policy/review');
    const logo = page.locator('.titlebar .ax-app-wordmark');
    await expect(logo).toHaveAttribute('href', '/');
    await expect(logo).toHaveAccessibleName('Ax home');
    const style = await logo.evaluate(async (el) => {
        await document.fonts.ready;
        const s = getComputedStyle(el);
        return { ink: s.color, font: s.fontFamily, weight: s.fontWeight, spacing: parseFloat(s.letterSpacing) / parseFloat(s.fontSize), loaded: document.fonts.check('700 29px "Archivo Variable"'), underline: getComputedStyle(el.querySelector('span')).color };
    });
    expect(style.font).toContain('Archivo Variable');
    expect(style.weight).toBe('700');
    expect(style.ink).toBe('rgb(240, 243, 237)');
    expect(style.spacing).toBeCloseTo(-.085, 3);
    expect(style.loaded).toBe(true);
    expect(style.underline).toBe('rgb(185, 255, 106)');
    await logo.focus();
    await page.keyboard.press('Enter');
    await expect(page).toHaveURL(/\/(stats)?$/);
    await expect(page.getByRole('button', { name: 'Quality gates', exact: true })).toHaveCount(0);
});
test('removed Quality Gates URLs return home including hash bookmarks', async ({ page }) => {
    for (const path of ['/ship', '/ship?embed=1', '/#ship']) {
        await page.goto(path);
        await expect(page).toHaveURL(/\/stats$/);
        await expect(page.getByRole('heading', { name: 'Quality gates', exact: true })).toHaveCount(0);
        await expect(page.getByRole('navigation', { name: 'Main navigation' })).toHaveCount(1);
    }
});
test('Rules and Skills tags fit their labels while semantic badges stay aligned', async ({ page }) => {
    const row = { id: 'tag-width', name: 'tag-width', level: 'INFO', scope: 'project', body: 'Tag width fixture', enabled: true, status: 'approved', alwaysApply: true, priority: 1, tags: ['graph', 'methodology'], globs: [], triggers: [], origin: 'project', sourcePath: 'rules/tag-width.mdc', effectiveStorage: 'files' };
    await page.route(/\/api\/policy\/rules(?:\?|$)/, route => route.fulfill({ json: { rules: [row] } }));
    await page.route(/\/api\/policy\/skills(?:\?|$)/, route => route.fulfill({ json: { skills: [row] } }));
    for (const mode of ['azure-light', 'azure-dark']) {
        await page.goto('/stats');
        await page.evaluate(id => localStorage.setItem('ax-theme', id), mode);
        for (const path of ['/policy/rules', '/policy/skills']) {
            await page.goto(path);
            const tags = page.locator('.policy-calm-row .policy-view-tags .page-item-badge');
            await expect(tags).toHaveCount(2);
            const widths = await tags.evaluateAll(es => es.map(e => e.getBoundingClientRect().width));
            expect(widths[0]).toBeLessThan(90);
            expect(widths[1]).toBeGreaterThan(widths[0] + 20);
            await expect(page.locator('.policy-calm-row .portal-data-cell .page-scope-badge')).toHaveCSS('width', '100px');
            await tags.first().click();
            await expect(page.locator('.policy-calm-row .policy-view-tags .page-item-badge').first()).toHaveClass(/page-item-badge--active/);
        }
    }
});
//# sourceMappingURL=admin-brand-home.spec.js.map