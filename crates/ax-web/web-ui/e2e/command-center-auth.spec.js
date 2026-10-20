import { expect, test } from '@playwright/test';
test('unauthenticated users see a sign-in screen, not project data', async ({ page }) => {
    let projectRequests = 0;
    await page.route('**/api/**', route => {
        projectRequests++;
        return route.fulfill({ status: 401, json: { error: 'Sign in required' } });
    });
    await page.route('**/api/web-auth/status', route => route.fulfill({ json: { enabled: true, passkeys: true } }));
    await page.route('**/api/web-auth/session', route => route.fulfill({ status: 401, json: { error: 'Sign in required' } }));
    await page.goto('/');
    await expect(page.getByRole('heading', { name: 'Sign in' })).toBeVisible();
    await expect(page.getByLabel('User name or email')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Sign in with passkey' })).toBeVisible();
    await expect(page.getByRole('navigation', { name: 'Main navigation' })).toHaveCount(0);
    expect(projectRequests).toBe(0);
});
test('password login unlocks the existing Command Center shell', async ({ page }) => {
    let authenticated = false;
    await page.route('**/api/**', route => route.fulfill({ status: 404, json: { error: 'Not provided in fixture' } }));
    await page.route('**/api/web-auth/status', route => route.fulfill({ json: { enabled: true, passkeys: true } }));
    await page.route('**/api/web-auth/session', route => route.fulfill(authenticated
        ? { json: { authenticated: true, member: { display_name: 'Owner', roles: ['owner'] } } }
        : { status: 401, json: { error: 'Sign in required' } }));
    await page.route('**/api/web-auth/login', async (route) => {
        expect(route.request().postDataJSON()).toEqual({ identity: 'owner', password: 'sample-strong-password' });
        authenticated = true;
        await route.fulfill({ json: { authenticated: true } });
    });
    await page.goto('/');
    await page.getByLabel('User name or email').fill('owner');
    await page.getByLabel('Password').fill('sample-strong-password');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await expect(page.getByRole('navigation', { name: 'Main navigation' })).toBeVisible();
});
test('invalid credentials stay on sign-in without revealing account details', async ({ page }) => {
    await page.route('**/api/web-auth/status', route => route.fulfill({ json: { enabled: true, passkeys: true } }));
    await page.route('**/api/web-auth/session', route => route.fulfill({ status: 401, json: { error: 'Sign in required' } }));
    await page.route('**/api/web-auth/login', route => route.fulfill({ status: 401, json: { error: 'Invalid credentials' } }));
    await page.goto('/');
    await page.getByLabel('User name or email').fill('unknown');
    await page.getByLabel('Password').fill('wrong-password');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await expect(page.getByRole('alert')).toContainText('Invalid credentials');
    await expect(page.getByRole('heading', { name: 'Sign in' })).toBeVisible();
});
//# sourceMappingURL=command-center-auth.spec.js.map