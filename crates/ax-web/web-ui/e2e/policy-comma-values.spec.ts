import { expect, test, type Page } from '@playwright/test';

import { emptyQualitySnapshot } from '../src/lib/mcpQuality';

const rule = {
  frontmatter: {
    id: 'alpha',
    level: 'INFO',
    alwaysApply: false,
    enabled: true,
    priority: 50,
    globs: ['src/**/*.{ts,tsx}', '**/*.rs'],
    triggers: ['review'],
    tags: ['misc', 'a,b'],
    scope: 'project',
    status: 'approved',
  },
  body: 'Saved body of alpha.',
};

const skill = {
  frontmatter: {
    name: 'deploy',
    description: 'Ship it',
    triggers: ['deploy', 'ship {prod,staging}'],
    tags: ['release', 'x,y'],
    priority: 50,
    enabled: true,
    scope: 'project',
    status: 'approved',
  },
  body: 'Saved body of deploy.',
};

async function serve(page: Page, saved: Array<Record<string, unknown>>): Promise<void> {
  await page.route('**/api/**', (route) => {
    const req = route.request();
    const path = new URL(req.url()).pathname;
    if (req.method() === 'PUT' && (path === '/api/policy/rules/alpha' || path === '/api/policy/skills/deploy')) {
      const payload = req.postDataJSON() as Record<string, unknown>;
      saved.push(payload);
      return route.fulfill({ json: payload });
    }
    if (path === '/api/links') return route.fulfill({ json: { outgoing: [], backlinks: [] } });
    if (path === '/api/policy/rules') {
      return route.fulfill({ json: { rules: [{ ...rule.frontmatter, body: '', sourcePath: '.agents/rules/alpha.mdc' }] } });
    }
    if (path === '/api/policy/rules/alpha') return route.fulfill({ json: rule });
    if (path === '/api/policy/skills') {
      return route.fulfill({ json: { skills: [{ ...skill.frontmatter, body: '', sourcePath: '.agents/skills/deploy/SKILL.md' }] } });
    }
    if (path === '/api/policy/skills/deploy') return route.fulfill({ json: skill });
    if (path === '/api/stats') {
      return route.fulfill({ json: { project_name: 'Commas', node_count: 0, edge_count: 0, file_count: 0, languages: [], db_size_bytes: 0 } });
    }
    if (path === '/api/usage/mcp-quality') return route.fulfill({ json: { ...emptyQualitySnapshot(), projectLabel: 'Commas' } });
    if (path === '/api/ship/config') return route.fulfill({ json: { config: { ship: {}, ui: {} }, sonar: {} } });
    return route.fulfill({ json: {} });
  });
  await page.setViewportSize({ width: 1600, height: 900 });
}

test.describe('Policy values that contain a comma', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop blade');

  test('a rule saved without touching its globs keeps a brace glob whole', async ({ page }) => {
    const saved: Array<Record<string, unknown>> = [];
    await serve(page, saved);
    await page.goto('/policy/rules');
    await page.evaluate(() => localStorage.setItem('ax-web-policy-body-view', 'markdown'));
    await page.goto('/policy/rules?id=alpha');
    const blade = page.getByRole('complementary', { name: 'alpha' });
    const text = blade.locator('textarea.w-md-editor-text-input');
    await expect(text).toHaveValue('Saved body of alpha.');
    await text.fill('New body.');
    await blade.getByRole('button', { name: 'Save', exact: true }).click();
    await expect.poll(() => saved.length).toBe(1);
    const fm = saved[0].frontmatter as Record<string, unknown>;
    expect(fm.globs).toEqual(['src/**/*.{ts,tsx}', '**/*.rs']);
    expect(fm.triggers).toEqual(['review']);
    expect(fm.tags).toEqual(['misc', 'a,b']);
  });

  test('the rule properties show a brace glob as one chip and a typed brace glob stays whole', async ({ page }) => {
    const saved: Array<Record<string, unknown>> = [];
    await serve(page, saved);
    await page.goto('/policy/rules?id=alpha');
    const blade = page.getByRole('complementary', { name: 'alpha' });
    await blade.getByRole('button', { name: 'Properties' }).click();
    const dialog = page.getByRole('dialog', { name: /Properties/ });
    await expect(dialog.getByRole('button', { name: 'Remove src/**/*.{ts,tsx}' })).toBeVisible();
    const globs = dialog.getByRole('textbox', { name: 'Globs' });
    await globs.click();
    await page.keyboard.type('lib/*.{js,mjs}');
    await page.keyboard.press('Enter');
    await expect(dialog.getByRole('button', { name: 'Remove lib/*.{js,mjs}' })).toBeVisible();
    await dialog.getByRole('button', { name: 'Done' }).click();
    await blade.getByRole('button', { name: 'Save', exact: true }).click();
    await expect.poll(() => saved.length).toBe(1);
    const fm = saved[0].frontmatter as Record<string, unknown>;
    expect(fm.globs).toEqual(['src/**/*.{ts,tsx}', '**/*.rs', 'lib/*.{js,mjs}']);
  });

  test('a skill saved without touching its triggers and tags keeps them whole', async ({ page }) => {
    const saved: Array<Record<string, unknown>> = [];
    await serve(page, saved);
    await page.goto('/policy/skills');
    await page.evaluate(() => localStorage.setItem('ax-web-policy-body-view', 'markdown'));
    await page.goto('/policy/skills?name=deploy');
    const blade = page.getByRole('complementary', { name: 'deploy' });
    const text = blade.locator('textarea.w-md-editor-text-input');
    await expect(text).toHaveValue('Saved body of deploy.');
    await text.fill('New skill body.');
    await blade.getByRole('button', { name: 'Save', exact: true }).click();
    await expect.poll(() => saved.length).toBe(1);
    const fm = saved[0].frontmatter as Record<string, unknown>;
    expect(fm.triggers).toEqual(['deploy', 'ship {prod,staging}']);
    expect(fm.tags).toEqual(['release', 'x,y']);
  });

  test('the full-page rule editor keeps a brace glob whole on save', async ({ page }) => {
    const saved: Array<Record<string, unknown>> = [];
    await serve(page, saved);
    await page.goto('/policy/rules/edit?id=alpha&mode=edit');
    const globs = page.locator('.policy-editor-page input[placeholder="**/*.tsx, **/*.css"]');
    await expect(globs).toHaveValue('src/**/*.{ts,tsx}, **/*.rs');
    await page.locator('.policy-editor-page').getByRole('button', { name: 'Save', exact: true }).click();
    await expect.poll(() => saved.length).toBe(1);
    const fm = saved[0].frontmatter as Record<string, unknown>;
    expect(fm.globs).toEqual(['src/**/*.{ts,tsx}', '**/*.rs']);
  });

  test('the full-page skill editor keeps a brace trigger whole on save', async ({ page }) => {
    const saved: Array<Record<string, unknown>> = [];
    await serve(page, saved);
    await page.goto('/policy/skills/edit?name=deploy&mode=edit');
    const editorPage = page.locator('.policy-editor-page');
    const edit = editorPage.getByRole('button', { name: 'Edit', exact: true });
    if (await edit.isVisible().catch(() => false)) await edit.click();
    await editorPage.getByRole('button', { name: 'Save', exact: true }).click();
    await expect.poll(() => saved.length).toBe(1);
    const fm = saved[0].frontmatter as Record<string, unknown>;
    expect(fm.triggers).toEqual(['deploy', 'ship {prod,staging}']);
  });
});
