import { expect, test, type Page } from '@playwright/test';

import { emptyQualitySnapshot } from '../src/lib/mcpQuality';

const graphNode = (id: string) => ({
  key: `rule:project:${id}`,
  kind: 'rule',
  id,
  label: id,
  origin: 'project',
  target: id,
  tags: ['misc'],
});

const ruleDoc = (id: string) => ({
  frontmatter: {
    id,
    level: 'INFO',
    alwaysApply: true,
    enabled: true,
    priority: 50,
    globs: [],
    triggers: [],
    tags: ['misc'],
    scope: 'project',
    status: 'approved',
  },
  body: `Saved body of ${id}.`,
});

async function openGraphEditor(page: Page): Promise<void> {
  await page.route('**/api/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/links/graph') {
      return route.fulfill({ json: { nodes: [graphNode('alpha'), graphNode('beta')], edges: [] } });
    }
    if (path === '/api/links') return route.fulfill({ json: { outgoing: [], backlinks: [] } });
    if (path === '/api/policy/rules') {
      const row = { ...ruleDoc('alpha').frontmatter, body: '', sourcePath: '.agents/rules/alpha.mdc' };
      return route.fulfill({ json: { rules: [row] } });
    }
    if (path === '/api/stats') {
      return route.fulfill({ json: { project_name: 'Graph', node_count: 0, edge_count: 0, file_count: 0, languages: [], db_size_bytes: 0 } });
    }
    if (path === '/api/usage/mcp-quality') return route.fulfill({ json: { ...emptyQualitySnapshot(), projectLabel: 'Graph' } });
    if (path === '/api/ship/config') return route.fulfill({ json: { config: { ship: {}, ui: {} }, sonar: {} } });
    const m = path.match(/^\/api\/policy\/rules\/([a-z-]+)$/);
    if (m) return route.fulfill({ json: ruleDoc(m[1]) });
    return route.fulfill({ json: {} });
  });
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto('/policy/rules');
  await page.evaluate(() => localStorage.setItem('ax-web-policy-body-view', 'markdown'));
  await page.goto('/policy/rules?id=alpha');
  await page.locator('button.btn-subtle', { hasText: /^Graph$/ }).click();
}

const overlay = (page: Page) => page.locator('.policy-graph-overlay');
const editor = (page: Page) => overlay(page).getByRole('complementary', { name: 'alpha' });
const source = (page: Page) => editor(page).locator('textarea.w-md-editor-text-input');

test.describe('Policy graph unsaved edits', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop overlay');

  test('Escape inside the editor does not close the rule', async ({ page }) => {
    await openGraphEditor(page);
    await expect(source(page)).toHaveValue('Saved body of alpha.');
    await source(page).fill('Several paragraphs of new text.');
    await source(page).press('Escape');
    await expect(editor(page)).toBeVisible();
    await expect(source(page)).toHaveValue('Several paragraphs of new text.');
  });

  test('Escape outside the editor asks before discarding edits', async ({ page }) => {
    await openGraphEditor(page);
    await source(page).fill('Unsaved words.');
    await overlay(page).locator('.policy-graph-title').click();

    let asked = 0;
    page.once('dialog', (d) => {
      asked++;
      void d.dismiss();
    });
    await page.keyboard.press('Escape');
    await expect.poll(() => asked).toBe(1);
    await expect(editor(page)).toBeVisible();
    await expect(source(page)).toHaveValue('Unsaved words.');

    page.once('dialog', (d) => void d.accept());
    await page.keyboard.press('Escape');
    await expect(editor(page)).toHaveCount(0);
  });

  test('Back asks before discarding edits', async ({ page }) => {
    await openGraphEditor(page);
    await source(page).fill('Unsaved words.');
    page.once('dialog', (d) => void d.dismiss());
    await editor(page).getByRole('button', { name: '← Back' }).click();
    await expect(source(page)).toHaveValue('Unsaved words.');
  });

  test('a clean rule closes without a prompt', async ({ page }) => {
    await openGraphEditor(page);
    await expect(source(page)).toHaveValue('Saved body of alpha.');
    let asked = 0;
    page.on('dialog', (d) => {
      asked++;
      void d.accept();
    });
    await overlay(page).locator('.policy-graph-title').click();
    await page.keyboard.press('Escape');
    await expect(editor(page)).toHaveCount(0);
    expect(asked).toBe(0);
  });
});

test('a skill with unsaved edits asks before Back closes it', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop overlay');
  const skill = {
    frontmatter: { name: 'deploy', description: 'Ship it', triggers: ['deploy'], tags: ['release'], priority: 50, enabled: true, scope: 'project', status: 'approved' },
    body: 'Saved body of deploy.',
  };
  await page.route('**/api/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/links/graph') {
      return route.fulfill({ json: { nodes: [{ key: 'skill:project:deploy', kind: 'skill', id: 'deploy', label: 'deploy', origin: 'project', target: 'deploy', tags: [] }], edges: [] } });
    }
    if (path === '/api/links') return route.fulfill({ json: { outgoing: [], backlinks: [] } });
    if (path === '/api/policy/skills') {
      return route.fulfill({ json: { skills: [{ ...skill.frontmatter, body: '', sourcePath: '.agents/skills/deploy/SKILL.md' }] } });
    }
    if (path === '/api/policy/skills/deploy') return route.fulfill({ json: skill });
    if (path === '/api/stats') {
      return route.fulfill({ json: { project_name: 'Graph', node_count: 0, edge_count: 0, file_count: 0, languages: [], db_size_bytes: 0 } });
    }
    if (path === '/api/usage/mcp-quality') return route.fulfill({ json: { ...emptyQualitySnapshot(), projectLabel: 'Graph' } });
    if (path === '/api/ship/config') return route.fulfill({ json: { config: { ship: {}, ui: {} }, sonar: {} } });
    return route.fulfill({ json: {} });
  });
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto('/policy/skills');
  await page.evaluate(() => localStorage.setItem('ax-web-policy-body-view', 'markdown'));
  await page.goto('/policy/skills?name=deploy');
  await page.locator('button.btn-subtle', { hasText: /^Graph$/ }).click();
  const skillEditor = overlay(page).getByRole('complementary', { name: 'deploy' });
  const text = skillEditor.locator('textarea.w-md-editor-text-input');
  await expect(text).toHaveValue('Saved body of deploy.');
  await text.fill('Unsaved skill words.');
  let asked = 0;
  page.once('dialog', (d) => {
    asked++;
    void d.dismiss();
  });
  await skillEditor.getByRole('button', { name: '← Back' }).click();
  await expect.poll(() => asked).toBe(1);
  await expect(text).toHaveValue('Unsaved skill words.');
});
