import { expect, test } from '@playwright/test';
async function rows(page, kind) {
    const res = await page.request.get(`/api/policy/${kind}`);
    const body = (await res.json());
    return body[kind];
}
const key = (r) => r.name ?? r.id ?? '';
/** A global item the server only finds when told its origin: the case that showed "not found". */
async function strictGlobal(page, kind, all) {
    for (const r of all.filter((x) => x.origin === 'global')) {
        const res = await page.request.get(`/api/policy/${kind}/${encodeURIComponent(key(r))}`);
        if (res.status() === 404)
            return r;
    }
    return undefined;
}
function hrefFor(kind, r) {
    const p = new URLSearchParams({ [kind === 'skills' ? 'name' : 'id']: key(r) });
    if (r.origin === 'global') {
        p.set('origin', 'global');
        if (r.projectId != null)
            p.set('projectId', String(r.projectId));
    }
    return `/policy/${kind}?${p}`;
}
async function go(page, href) {
    await page.evaluate((h) => {
        window.history.pushState(null, '', h);
        window.dispatchEvent(new PopStateEvent('popstate'));
    }, href);
}
const blade = (page, name) => page.getByRole('complementary', { name });
test.describe('Links open the right rule or skill', () => {
    test.skip(({ isMobile }) => isMobile, 'desktop blade');
    for (const kind of ['skills', 'rules']) {
        test(`L1 a ${kind} link with origin=global opens the global item`, async ({ page }) => {
            await page.goto(`/policy/${kind}`);
            const global = await strictGlobal(page, kind, await rows(page, kind));
            test.skip(!global, `no project-scoped global ${kind} on this machine`);
            await go(page, hrefFor(kind, global));
            await expect(blade(page, key(global))).toBeVisible();
            await expect(blade(page, key(global)).getByText('not found', { exact: false })).toHaveCount(0);
            await expect(blade(page, key(global)).getByRole('group', { name: 'Body view' })).toBeVisible();
        });
        test(`L2 after a global ${kind} item, a link to a project item still opens`, async ({ page }) => {
            await page.goto(`/policy/${kind}`);
            const all = await rows(page, kind);
            const global = await strictGlobal(page, kind, all);
            const local = all.find((r) => r.origin !== 'global');
            test.skip(!global || !local, `needs a global and a project ${kind}`);
            await go(page, hrefFor(kind, global));
            await expect(blade(page, key(global)).getByRole('group', { name: 'Body view' })).toBeVisible();
            await go(page, hrefFor(kind, local));
            await expect(blade(page, key(local)).getByRole('group', { name: 'Body view' })).toBeVisible();
            await expect(blade(page, key(local)).getByText('not found', { exact: false })).toHaveCount(0);
        });
        test(`L3 a ${kind} link without origin to an item that only exists globally still opens`, async ({ page }) => {
            await page.goto(`/policy/${kind}`);
            const all = await rows(page, kind);
            const onlyGlobal = await strictGlobal(page, kind, all.filter((r) => !all.some((o) => o.origin !== 'global' && key(o) === key(r))));
            test.skip(!onlyGlobal, `no global-only ${kind}`);
            await go(page, `/policy/${kind}?${kind === 'skills' ? 'name' : 'id'}=${encodeURIComponent(key(onlyGlobal))}`);
            await expect(blade(page, key(onlyGlobal)).getByRole('group', { name: 'Body view' })).toBeVisible();
        });
        test(`L4 clicking a global ${kind} row puts its origin in the URL`, async ({ page }) => {
            await page.goto(`/policy/${kind}`);
            const global = await strictGlobal(page, kind, await rows(page, kind));
            test.skip(!global, `no project-scoped global ${kind} on this machine`);
            await page.getByRole('main').getByText(key(global), { exact: true }).first().click();
            await expect(page).toHaveURL(/origin=global/);
            if (global.projectId != null)
                await expect(page).toHaveURL(new RegExp(`projectId=${global.projectId}`));
            await expect(blade(page, key(global)).getByRole('group', { name: 'Body view' })).toBeVisible();
        });
    }
});
//# sourceMappingURL=policy-link-origin.spec.js.map