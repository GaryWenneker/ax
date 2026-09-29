import { expect, test, type APIRequestContext } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const image = path.join(repo, 'docs/specs/policy-graph.png');
const commit = execFileSync('git', ['-C', repo, 'rev-parse', '--short=9', 'b7caeb1']).toString().trim();
const file = 'docs/specs/calm-list-style.md';

async function createMemory(request: APIRequestContext, title: string): Promise<string> {
  const res = await request.post('/api/memory', {
    data: {
      title,
      body: `${title} ![shot](${image})\n\nCommits:\n- ${commit} Spec: calm list style`,
      kind: 'note',
      tags: [],
      files: [file],
    },
  });
  expect(res.ok()).toBeTruthy();
  return (await res.json()).memory.id as string;
}

test.describe('Memory blade files', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop hover');
  let id = '';
  const title = `e2e memory files ${Date.now()}`;

  test.beforeAll(async ({ request }) => {
    id = await createMemory(request, title);
  });

  test.afterAll(async ({ request }) => {
    if (id) await request.delete(`/api/memory/${encodeURIComponent(id)}`);
  });

  test('file rows show their change, open the diff on click only, and images render', async ({ page }) => {
    await page.goto('/memory');
    await page.getByPlaceholder(/^Recall/).fill(title);
    await page.getByText(title).first().click();
    const blade = page.locator('aside.memory-blade');
    await expect(blade).toBeVisible();

    const row = blade.locator('.memory-file', { hasText: file });
    await expect(row).toHaveClass(/memory-file--added/);

    const diff = page.getByRole('dialog', { name: new RegExp(file.replace(/[.]/g, '\\.')) });
    await row.hover();
    await page.waitForTimeout(700);
    await expect(diff).toHaveCount(0);

    await row.click();
    await expect(diff).toBeVisible();
    await expect(diff.locator('.memory-diff-line').first()).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(diff).toHaveCount(0);
    await expect(blade).toBeVisible();

    const img = blade.locator('.memory-md img').first();
    await expect(img).toBeVisible();
    await expect.poll(() => img.evaluate((el: HTMLImageElement) => el.naturalWidth)).toBeGreaterThan(0);
  });
});
