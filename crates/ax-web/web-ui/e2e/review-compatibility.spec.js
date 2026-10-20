import { test, expect } from '@playwright/test';
test('large queues use compact rows and full comparison remains reachable', async ({ page }) => {
    const items = Array.from({ length: 80 }, (_, i) => ({ id: `replica-${String(i).padStart(64, '0')}`, kind: 'rule', effect: 'replacement', scope: 'project', path: 'pending/rule.mdc', status: 'pending', preview: 'Incoming preview', incomingContent: 'Full incoming policy body', currentContent: 'Full current policy body', levelOrDescription: 'Long description '.repeat(25), origin: 'Primary fixture', changedFields: ['body'] }));
    await page.route('**/api/policy/review', r => r.fulfill({ json: { items, serverVersion: '8.1.0', comparisonAvailable: true } }));
    await page.goto('/policy/review');
    await expect(page.locator('.review-item')).toHaveCount(80);
    const row = page.locator('.review-item').first();
    const height = (await row.boundingBox())?.height;
    console.log(`Review row height (${test.info().project.name}): ${height}px`);
    if (process.env.AX_REVIEW_SCREENSHOTS)
        await page.screenshot({ path: `../../../site/public/screenshots/cc-review-compact-${test.info().project.name === 'mobile-chrome' ? 'mobile' : 'desktop'}.png` });
    expect(height).toBeLessThan(test.info().project.name === 'mobile-chrome' ? 150 : 95);
    await row.getByText('Inspect content and source', { exact: true }).click();
    await expect(row.getByText('Full current policy body', { exact: true })).toBeVisible();
    await expect(row.getByText('Full incoming policy body', { exact: true })).toBeVisible();
    if (process.env.AX_REVIEW_SCREENSHOTS && test.info().project.name === 'desktop-chrome')
        await row.screenshot({ path: '../../../site/public/screenshots/cc-review-comparison.png' });
});
test('legacy comparison responses show one clear warning and block approval', async ({ page }) => {
    const items = Array.from({ length: 80 }, (_, i) => ({ id: `legacy-${i}`, kind: 'rule', path: 'pending/rule.mdc', status: 'pending', preview: 'Partial preview', levelOrDescription: 'CRITICAL' }));
    items[0] = { ...items[0], effect: 'replacement', incomingContent: 'Incoming without verified current' };
    await page.route('**/api/policy/review', r => r.fulfill({ json: { items } }));
    await page.goto('/policy/review');
    await expect(page.getByRole('alert').filter({ hasText: 'does not provide full policy comparisons' })).toHaveCount(1);
    await expect(page.locator('.review-item').first().getByRole('button', { name: 'Approve', exact: true })).toBeDisabled();
    await expect(page.getByRole('button', { name: 'Approve filtered type', exact: true })).toBeDisabled();
});
test('persisted peer warnings are visible in replication settings', async ({ page }) => {
    await page.route('**/api/access/nodes/local', r => r.fulfill({ json: { settings: { config: { node_id: 'replica', role: 'replica', cluster_id: 'team', primary_url: 'https://primary.example.test', projects: [{ id: 'project' }] }, port: 7071 }, running: true, active_project: true, active_replicas: 0, sync: { status: 'full_data_synced', compatibility_warning: 'Peer Ax 8.0.0; local Ax 8.1.0. Protocol 3 is compatible.', last_success_unix_seconds: 1791800000 }, safety: { mode: 'preserve', memories: 1, rules: 1, skills: 1 } } }));
    await page.goto('/settings?section=nodes');
    await expect(page.getByText('Peer Ax 8.0.0; local Ax 8.1.0. Protocol 3 is compatible.', { exact: true })).toBeVisible();
});
//# sourceMappingURL=review-compatibility.spec.js.map