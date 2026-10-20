import { expect, test } from '@playwright/test';
import { chooseSelect } from './chooseSelect';
import { readFileSync } from 'node:fs';
if (process.env.AX_CHROME_EXECUTABLE)
    test.use({ launchOptions: { executablePath: process.env.AX_CHROME_EXECUTABLE } });
const endpoint = 'https://ax.example.com/projects/remote-demo/mcp';
function publication(id = 'publication', label = 'Team Ax', url = endpoint) {
    return { id, publisherId: id, publisherLabel: label, projectLabel: 'Demo project', endpointUrl: url,
        publicOrigin: 'https://ax.example.com', oauthIssuer: 'https://issuer.example.com/', resource: 'https://ax.example.com',
        remoteProjectId: 'remote-demo', configFingerprint: 'config-revision', runtimeStatus: 'stopped', lastHeartbeatAt: null };
}
async function fixture(page, options = {}) {
    let workspaceId = 'workspace-one';
    let publications = options.empty ? [] : options.multiple ? [publication(), publication('second', 'Office Ax', 'https://office.example/projects/remote-demo/mcp')] : [publication()];
    const requests = [];
    await page.route('**/api/**', async (route) => {
        const request = route.request();
        const path = new URL(request.url()).pathname;
        const method = request.method();
        const body = request.postData() ? request.postDataJSON() : null;
        requests.push({ path, method, body });
        let value;
        if (path === '/api/remote-mcp/publications/')
            value = { workspaceId, projectLabel: 'Demo project', csrf: 'fixture-csrf', publications };
        else if (path.endsWith('/host'))
            value = { workspaceId, host: null };
        else if (path.endsWith('/host/authorizations'))
            value = { workspaceId, requests: [] };
        else if (path.endsWith('/local/export')) {
            const args = request.postDataJSON(); // Fixture mirrors validated local request fields.
            const shared = args.target === 'projectFolder';
            const server = { command: shared ? 'ax' : '/tools/ax', args: ['serve', '--mcp', '--path', shared ? '${workspaceFolder}' : '/demo'] };
            value = { workspaceId, client: args.client, target: args.target, fileName: 'mcp.json', destination: args.client === 'vscode' ? '.vscode/mcp.json' : '.cursor/mcp.json', content: JSON.stringify({ [args.client === 'vscode' ? 'servers' : 'mcpServers']: { ax: server } }, null, 2), instructions: ['Paste into the app settings and reload.'], containsSecrets: false, containsLocalPaths: !shared, portable: shared };
        }
        else if (path.endsWith('/connection')) {
            value = { workspaceId, publicationId: 'publication', configFingerprint: 'config-revision', content: JSON.stringify({ kind: 'axRemoteConnection', version: 1, projectMcpUrl: endpoint, nativeClientId: 'native-public' }), containsSecrets: false, requiresClientId: false };
        }
        else if (path.endsWith('/export')) {
            expect(request.headers()['x-ax-csrf']).toBe('fixture-csrf');
            const args = request.postDataJSON(); // Fixture mirrors the typed export request.
            const capturedWorkspace = workspaceId;
            const selected = publications.find(p => path.split('/').at(-2) === p.id) ?? publication();
            let server = args.transport === 'proxy' ? { command: 'ax', args: ['mcp', 'remote', 'proxy', '--url', selected.endpointUrl] } : { url: selected.endpointUrl };
            if (args.client === 'vscode' || args.client === 'claudeCode')
                server.type = args.transport === 'proxy' ? 'stdio' : 'http';
            if (args.client === 'cline') {
                if (args.transport === 'direct')
                    server.type = 'streamableHttp';
                server = { ...server, disabled: false, autoApprove: [] };
            }
            value = { workspaceId: capturedWorkspace, publicationId: selected.id, configFingerprint: args.configFingerprint, client: args.client,
                serverKey: 'axRemoteDemo', format: args.client === 'chatgpt' ? 'url' : 'json', fileName: args.client === 'chatgpt' ? null : 'mcp.json',
                content: args.client === 'chatgpt' ? selected.endpointUrl : JSON.stringify({ [args.client === 'vscode' ? 'servers' : 'mcpServers']: { axRemoteDemo: server } }, null, 2),
                instructions: ['Merge this entry into your receiving client settings.'], requirements: options.noNative && args.transport === 'proxy' ? ['Register a native Ax OAuth client first.'] : [],
                loginCommand: options.noNative ? null : `ax mcp remote login --url '${selected.endpointUrl}' --client-id 'native-public'`, containsSecrets: false };
            if (options.delayExport)
                await new Promise(resolve => setTimeout(resolve, 350));
        }
        else if (path.endsWith('/check'))
            value = { workspaceId, publicationId: 'publication', configFingerprint: 'config-revision', state: 'signInRequired', message: 'Endpoint reachable, sign-in required. Project access has not been verified.' };
        else if (path.endsWith('/import')) {
            publications = [publication()];
            value = { workspaceId, registered: true };
        }
        else if (path.startsWith('/api/remote-mcp/publications/') && method === 'DELETE') {
            publications = [];
            value = { workspaceId, removed: true };
        }
        else if (path === '/api/remote-mcp/')
            value = { csrf: 'fixture-csrf', connections: [] };
        else if (path === '/api/share/status')
            value = { readonly: false, sharing: false, port: 7070 };
        else if (path === '/api/stats')
            value = { readonly: false, project_name: 'Demo project', node_count: 0, edge_count: 0, file_count: 0, db_size_bytes: 0, languages: [] };
        else if (path === '/api/ship/config')
            value = { config: { ship: { target_branch: 'main', web_port: 7070 }, remote: { provider: 'github' }, quality_gate: { steps: [], tests: {} }, sonar: {}, ui: {} } };
        else if (path === '/api/ship/review-language')
            value = { code: 'en', languages: [] };
        else if (path === '/api/workspace/current')
            value = { ok: true, workspace: { path: '/demo', label: 'Demo project', initialized: true }, recent: [] };
        await route.fulfill({ status: value === undefined ? 404 : 200, json: value ?? { error: 'Fixture endpoint not provided' } }).catch(() => { });
    });
    await page.addInitScript(({ denied }) => {
        localStorage.setItem('ax-theme', 'azure-light');
        localStorage.setItem('ax-theme-mint-v2', '1');
        Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async (value) => {
                    if (denied)
                        throw new DOMException('Denied', 'NotAllowedError');
                    localStorage.setItem('fixture-copied', value);
                } } });
    }, { denied: options.clipboardDenied ?? false });
    return { requests, switchProject: async () => { workspaceId = 'workspace-two'; publications = []; await page.evaluate(() => window.dispatchEvent(new CustomEvent('ax-workspace-switched'))); } };
}
test('copies and downloads the exact client JSON and distinguishes endpoint reachability from login', async ({ page }, info) => {
    await fixture(page);
    await page.goto('/settings?section=remote');
    const section = page.locator('.remote-mcp-publish');
    await expect(section.getByRole('heading', { name: 'Share this project' })).toBeVisible();
    await expect(section).toContainText('Server stopped');
    await expect(section.getByLabel('Generated MCP configuration')).toContainText(endpoint);
    await chooseSelect(section.getByLabel('MCP client'), 'vscode');
    const preview = section.getByLabel('Generated MCP configuration');
    await expect(preview).toContainText('"servers"');
    const content = await preview.inputValue();
    await section.getByRole('button', { name: 'Copy configuration', exact: true }).click();
    await expect(section.locator('.remote-publish-feedback')).toHaveText('Configuration copied.');
    expect(await page.evaluate(() => localStorage.getItem('fixture-copied'))).toBe(content);
    const downloadPromise = page.waitForEvent('download');
    await section.getByRole('button', { name: 'Download configuration' }).click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toBe('mcp.json');
    expect(readFileSync((await download.path()), 'utf8')).toBe(content);
    await section.getByRole('button', { name: 'Verify endpoint' }).click();
    await expect(section).toContainText('Endpoint reachable, sign-in required. Project access has not been verified.');
    await section.getByRole('button', { name: 'Copy MCP URL' }).click();
    await expect(section.getByRole('status').filter({ hasText: 'MCP URL copied.' })).toBeVisible();
    expect(await page.evaluate(() => localStorage.getItem('fixture-copied'))).toBe(endpoint);
    await chooseSelect(section.getByLabel('MCP client'), 'chatgpt');
    await expect(preview).toHaveValue(endpoint);
    await expect(section.getByRole('button', { name: 'Download configuration' })).toHaveCount(0);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    if (process.env.AX_REMOTE_SCREENSHOTS) {
        await chooseSelect(section.getByLabel('MCP client'), 'cursor');
        await expect(preview).toContainText('mcpServers');
        await page.setViewportSize({ width: info.project.name.startsWith('mobile') ? 393 : 1280, height: info.project.name.startsWith('mobile') ? 1700 : 1150 });
        await page.evaluate(() => document.querySelectorAll('*').forEach(el => { if (el instanceof HTMLElement && el.scrollTop)
            el.scrollTop = 0; }));
        await page.screenshot({ scale: 'css', path: `../../../site/public/screenshots/remote-mcp-project-${info.project.name.startsWith('mobile') ? 'mobile' : 'desktop'}.png`, fullPage: true });
    }
});
test('requires publisher selection and uses Cline proxy with no invented native login command', async ({ page }) => {
    await fixture(page, { multiple: true, noNative: true });
    await page.goto('/settings?section=remote');
    const section = page.locator('.remote-mcp-publish');
    await expect(section.getByLabel('Publisher')).toHaveAttribute('data-value', '');
    await expect(section.getByLabel('Generated MCP configuration')).toHaveCount(0);
    await chooseSelect(section.getByLabel('Publisher'), 'second');
    await expect(section.getByLabel('Generated MCP configuration')).toContainText('office.example');
    await chooseSelect(section.getByLabel('MCP client'), 'cline');
    await expect(section.getByLabel('MCP transport')).toHaveAttribute('data-value', 'proxy');
    await expect(section.getByLabel('Generated MCP configuration')).toContainText('"command": "ax"');
    await expect(section.getByRole('button', { name: 'Copy login command (POSIX)' })).toBeDisabled();
    await chooseSelect(section.getByLabel('MCP transport'), 'direct');
    await expect(section.getByLabel('Generated MCP configuration')).toContainText('streamableHttp');
});
test('clipboard denial selects manual text without a copied confirmation', async ({ page }) => {
    await fixture(page, { clipboardDenied: true });
    await page.goto('/settings?section=remote');
    const section = page.locator('.remote-mcp-publish');
    await section.getByRole('button', { name: 'Copy configuration', exact: true }).click();
    await expect(section.getByRole('alert')).toContainText('Clipboard access was denied');
    await expect(section.locator('.remote-publish-feedback')).toHaveCount(0);
    await expect(section.getByLabel('Generated MCP configuration')).toBeFocused();
    const selected = await section.getByLabel('Generated MCP configuration').evaluate(e => { if (!(e instanceof HTMLTextAreaElement))
        throw new Error('Expected textarea'); return e.selectionEnd - e.selectionStart; });
    expect(selected).toBeGreaterThan(20);
});
test('workspace switch clears old configuration and ignores a late response in embedded IDE mode', async ({ page }) => {
    const f = await fixture(page, { delayExport: true });
    await page.goto('/settings?section=remote&embed=1');
    await expect(page.locator('html')).toHaveAttribute('data-ax-embed', '1');
    await expect.poll(() => f.requests.some(r => r.path.endsWith('/export') && !r.path.endsWith('/local/export'))).toBe(true);
    await f.switchProject();
    await expect(page.locator('.remote-mcp-publish')).toContainText('No remote server is registered');
    await page.waitForTimeout(450);
    await expect(page.getByLabel('Generated MCP configuration')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Copy MCP URL' })).toHaveCount(0);
});
test('registration opens a modal, sends only a JSON object and removes only the local registration', async ({ page }, info) => {
    const f = await fixture(page, { empty: true });
    await page.goto('/settings?section=remote');
    await page.getByText('Advanced: register an externally managed server', { exact: true }).click();
    await page.getByRole('button', { name: 'Import existing server file' }).click();
    const modal = page.getByRole('dialog', { name: 'Import existing remote server' });
    await expect(modal).toBeVisible();
    await expect(modal.getByLabel('Native Ax public client ID (optional)')).not.toBeVisible();
    if (process.env.AX_REMOTE_SCREENSHOTS && info.project.name === 'desktop-chrome')
        await page.screenshot({ path: '../../../site/public/screenshots/mcp-import-existing-server.png', scale: 'css' });
    await modal.getByLabel('Server name (optional)').fill('Team Ax');
    await modal.getByLabel('Server configuration file').setInputFiles({ name: 'remote.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify({ public_url: 'https://ax.example.com', oauth: { issuer: 'https://issuer.example/', jwks_uri: 'https://issuer.example/jwks' }, projects: [{ id: 'remote-demo', root: '/demo', subjects: ['subject'] }] })) });
    await modal.getByRole('button', { name: 'Import for this project' }).click();
    await expect(modal).toHaveCount(0);
    const imported = f.requests.find(r => r.path.endsWith('/import'));
    expect(imported?.body).toMatchObject({ workspaceId: 'workspace-one', name: 'Team Ax', config: { public_url: 'https://ax.example.com' } });
    await page.getByRole('button', { name: 'Remove local registration' }).click();
    await expect(page.locator('.remote-mcp-publish')).toContainText('No remote server is registered');
    expect(f.requests.filter(r => r.method === 'DELETE').map(r => r.path)).toEqual(['/api/remote-mcp/publications/publication']);
});
test('local configuration is copyable without a remote server and project sharing has no host paths', async ({ page }, info) => {
    await fixture(page, { empty: true });
    await page.goto('/settings?section=remote');
    const local = page.locator('.local-mcp-config');
    await expect(local.getByRole('button', { name: 'Copy for this computer' })).toBeEnabled();
    if (process.env.AX_REMOTE_SCREENSHOTS && info.project.name === 'desktop-chrome') {
        await page.setViewportSize({ width: 1280, height: 1150 });
        await page.screenshot({ path: '../../../site/public/screenshots/mcp-local-and-remote.png', scale: 'css' });
    }
    await local.getByRole('button', { name: 'Copy for this computer' }).click();
    await expect(local.getByRole('status')).toHaveText('Configuration for this computer copied.');
    expect(await page.evaluate(() => localStorage.getItem('fixture-copied'))).toContain('/demo');
    await local.getByRole('button', { name: 'Copy to share with project' }).click();
    await expect(local.getByRole('status')).toHaveText('Shareable project configuration copied.');
    const shared = await page.evaluate(() => localStorage.getItem('fixture-copied'));
    expect(shared).toContain('${workspaceFolder}');
    expect(shared).not.toContain('/demo');
    expect(shared).not.toContain('/tools/ax');
    await page.getByText('Advanced: register an externally managed server', { exact: true }).click();
    await expect(page.getByRole('button', { name: 'Import existing server file' })).toBeVisible();
});
test('remote connection details can be copied and pasted here without starting sign-in automatically', async ({ page }) => {
    const f = await fixture(page);
    await page.goto('/settings?section=remote');
    await page.getByRole('button', { name: 'Copy connection details', exact: true }).click();
    await expect(page.locator('.remote-publish-feedback').filter({ hasText: 'Connection details copied.' })).toBeVisible();
    const copied = await page.evaluate(() => localStorage.getItem('fixture-copied'));
    await page.getByRole('button', { name: 'Paste connection details' }).click();
    const dialog = page.getByRole('dialog', { name: 'Paste Ax connection details' });
    await dialog.getByLabel('Received Ax connection details').fill(copied);
    await dialog.getByRole('button', { name: 'Use connection' }).click();
    await expect(dialog).toHaveCount(0);
    const connect = page.locator('.remote-mcp-settings');
    await expect(connect.getByLabel('Project MCP URL')).toHaveValue(endpoint);
    await expect(connect.getByLabel('OAuth client ID')).toHaveValue('native-public');
    await expect(connect.getByRole('button', { name: 'Connect with browser' })).toBeEnabled();
    expect(f.requests.some(r => r.path.endsWith('/login'))).toBe(false);
});
//# sourceMappingURL=remote-mcp-publish.spec.js.map