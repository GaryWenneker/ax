import { expect, test, type Page } from '@playwright/test';

import { emptyQualitySnapshot } from '../src/lib/mcpQuality';

const items = [
  { kind: 'rule', id: 'alpha', status: 'conflict', compare: 'differs' },
  { kind: 'rule', id: 'beta', status: 'conflict', compare: 'differs' },
];

const hunk = (index: number, word: string) => ({ index, local: [`local ${word} ${index}`], package: [`package ${word} ${index}`] });

const diffs: Record<string, unknown> = {
  alpha: { kind: 'rule', id: 'alpha', compare: 'differs', unified: '', hunks: [hunk(0, 'alpha'), hunk(1, 'alpha'), hunk(2, 'alpha')] },
  beta: { kind: 'rule', id: 'beta', compare: 'differs', unified: '', hunks: [hunk(0, 'beta')] },
};

async function serve(page: Page, restored: string[]): Promise<void> {
  await page.route('**/api/**', async (route) => {
    const req = route.request();
    const path = new URL(req.url()).pathname;
    if (path === '/api/policy/package/preview') return route.fulfill({ json: { name: 'pack', items } });
    if (path === '/api/policy/package/diff') {
      const form = req.postData() ?? '';
      const id = /name="id"\r\n\r\n([a-z]+)/.exec(form)?.[1] ?? '';
      // The first item answers late, so its reply lands after the second one.
      if (id === 'alpha') await new Promise((r) => setTimeout(r, 800));
      return route.fulfill({ json: diffs[id] ?? {} });
    }
    if (path === '/api/policy/package/restore') {
      restored.push(req.postData() ?? '');
      return route.fulfill({ json: { written: [], skipped: [], errors: [] } });
    }
    if (path === '/api/links') return route.fulfill({ json: { outgoing: [], backlinks: [] } });
    if (path === '/api/policy/rules') return route.fulfill({ json: { rules: [] } });
    if (path === '/api/stats') {
      return route.fulfill({ json: { project_name: 'Pack', node_count: 0, edge_count: 0, file_count: 0, languages: [], db_size_bytes: 0 } });
    }
    if (path === '/api/usage/mcp-quality') return route.fulfill({ json: { ...emptyQualitySnapshot(), projectLabel: 'Pack' } });
    if (path === '/api/ship/config') return route.fulfill({ json: { config: { ship: {}, ui: {} }, sonar: {} } });
    return route.fulfill({ json: {} });
  });
  await page.setViewportSize({ width: 1600, height: 900 });
}

test('a late diff reply for an earlier row does not replace the diff of the row shown', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop modal');
  const restored: string[] = [];
  await serve(page, restored);
  await page.goto('/policy/rules');
  await page.getByRole('button', { name: 'Restore package' }).click();
  await page.locator('#policy-pack-zip-input').setInputFiles({ name: 'pack.ax-policy.zip', mimeType: 'application/zip', buffer: Buffer.from('PK') });
  const rows = page.locator('.policy-pack-preview-row');
  await expect(rows).toHaveCount(2);
  await rows.filter({ hasText: 'alpha' }).click();
  await rows.filter({ hasText: 'beta' }).click();
  const pane = page.locator('.policy-pack-inspect');
  await expect(pane.locator('.policy-pack-inspect-title')).toContainText('rule:beta');
  await expect(pane).toContainText('package beta 0');
  // Give alpha's late reply time to land.
  await page.waitForTimeout(1200);
  await expect(pane).toContainText('package beta 0');
  await expect(pane).not.toContainText('alpha');
  await expect(pane.locator('.policy-pack-hunk')).toHaveCount(1);

  // A per-change pick is stored for beta from beta's own single change.
  await pane.locator('.policy-pack-hunk input[type="checkbox"]').nth(1).check();
  await page.getByRole('button', { name: 'Restore', exact: true }).click();
  await expect.poll(() => restored.length).toBe(1);
  const decisions = JSON.parse(/name="decisions"\r\n\r\n(.*)\r\n/.exec(restored[0])?.[1] ?? '{}') as Record<string, unknown>;
  const beta = decisions['rule:beta'] as { hunks?: unknown[] } | string;
  expect(typeof beta === 'object' && Array.isArray(beta.hunks) ? beta.hunks.length : -1).toBe(1);
});
