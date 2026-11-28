import { expect, test, type APIRequestContext } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const image = path.join(repo, 'docs/specs/policy-graph.png');
const knownCommit = 'b7caeb1';

/**
 * The short hash of a commit that added `file`, or null when this clone does not have it (a shallow CI
 * checkout, rewritten history, no git). Looked up when the tests run, never while Playwright collects specs.
 */
let lookedUp: string | null | undefined;
function historicalCommit(): string | null {
  if (lookedUp !== undefined) return lookedUp;
  try {
    lookedUp = execFileSync('git', ['-C', repo, 'rev-parse', '--verify', '--quiet', '--short=9', `${knownCommit}^{commit}`], { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim() || null;
  } catch {
    lookedUp = null;
  }
  return lookedUp;
}
const file = 'docs/specs/calm-list-style.md';

async function createMemory(request: APIRequestContext, title: string, commit: string): Promise<string> {
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
  test.skip(() => historicalCommit() === null, `commit ${knownCommit} is not in this clone`);
  let id = '';
  const title = `e2e memory files ${Date.now()}`;

  test.beforeAll(async ({ request }) => {
    const commit = historicalCommit();
    if (commit) id = await createMemory(request, title, commit);
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
