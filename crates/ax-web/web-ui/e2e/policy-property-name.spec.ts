import { expect, test } from '@playwright/test';

import { emptyQualitySnapshot } from '../src/lib/mcpQuality';

const frontmatter = {
  id: 'alpha',
  level: 'INFO',
  alwaysApply: true,
  enabled: true,
  priority: 50,
  globs: [],
  triggers: [],
  tags: ['misc'],
  scope: 'project',
  status: 'approved',
};

test('a new property name can be typed in one go', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop blade');
  await page.route('**/api/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/links') return route.fulfill({ json: { outgoing: [], backlinks: [] } });
    if (path === '/api/policy/rules') {
      return route.fulfill({ json: { rules: [{ ...frontmatter, body: '', sourcePath: '.agents/rules/alpha.mdc' }] } });
    }
    if (path === '/api/policy/rules/alpha') return route.fulfill({ json: { frontmatter, body: 'Body.' } });
    if (path === '/api/stats') {
      return route.fulfill({ json: { project_name: 'Props', node_count: 0, edge_count: 0, file_count: 0, languages: [], db_size_bytes: 0 } });
    }
    if (path === '/api/usage/mcp-quality') return route.fulfill({ json: { ...emptyQualitySnapshot(), projectLabel: 'Props' } });
    if (path === '/api/ship/config') return route.fulfill({ json: { config: { ship: {}, ui: {} }, sonar: {} } });
    return route.fulfill({ json: {} });
  });
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.goto('/policy/rules?id=alpha');
  const blade = page.getByRole('complementary', { name: 'alpha' });
  await blade.getByRole('button', { name: 'Properties' }).click();
  const dialog = page.getByRole('dialog', { name: /Properties/ });
  await dialog.getByRole('button', { name: 'Add property' }).click();
  const name = dialog.getByRole('textbox', { name: 'Property 1 name' });
  await expect(name).toBeFocused();
  await page.keyboard.type('owner');
  await expect(name).toHaveValue('owner');
  await expect(name).toBeFocused();
  await dialog.getByRole('textbox', { name: 'Property 1 value' }).fill('platform-team');
  await expect(dialog.getByRole('button', { name: 'owner', exact: true })).toBeVisible();
});
