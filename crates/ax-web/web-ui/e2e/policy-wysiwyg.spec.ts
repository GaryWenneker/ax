import { expect, test, type Page } from '@playwright/test';

const FIXTURE = [
  '# Title',
  '',
  'See [[wcag-contrast]] and [[global/rules/x|alias]].',
  '',
  '<!-- keep me -->',
  '',
  '| a | b |',
  '| --- | --- |',
  '| 1 | 2 |',
  '',
  '```rust',
  'fn main() {}',
  '```',
  '',
  '- [ ] todo',
  '- [x] done',
  '',
  'end',
].join('\n');

const blade = (page: Page) => page.getByRole('complementary', { name: 'english-only' });
const source = (page: Page) => blade(page).locator('textarea.w-md-editor-text-input');
const rich = (page: Page) => blade(page).locator('.wysiwyg-content');
const viewBtn = (page: Page, name: string) =>
  blade(page).getByRole('group', { name: 'Body view' }).getByRole('button', { name, exact: true });

async function openRule(page: Page, view: 'markdown' | 'wysiwyg' | 'preview') {
  await page.goto('/policy/rules');
  await page.evaluate((v) => localStorage.setItem('ax-web-policy-body-view', v), view);
  await page.goto('/policy/rules?id=english-only');
  await expect(blade(page)).toBeVisible();
}

test.describe('Policy body WYSIWYG', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop blade');

  test('W1 three views, the choice is remembered', async ({ page }) => {
    await openRule(page, 'preview');
    await viewBtn(page, 'WYSIWYG').click();
    await expect(rich(page)).toBeVisible();
    await expect(viewBtn(page, 'WYSIWYG')).toHaveAttribute('aria-pressed', 'true');
    await page.reload();
    await expect(rich(page)).toBeVisible();
  });

  test('W3 opening in WYSIWYG does not rewrite the body', async ({ page }) => {
    await openRule(page, 'markdown');
    const original = '* item\n\n__b__\n\n|a|b|\n|-|-|\n|1|2|';
    await source(page).fill(original);
    await viewBtn(page, 'WYSIWYG').click();
    await expect(rich(page)).toBeVisible();
    await page.waitForTimeout(500);
    await viewBtn(page, 'Markdown').click();
    await expect(source(page)).toHaveValue(original);
  });

  test('W5 switching items in WYSIWYG shows the new body unchanged', async ({ page }) => {
    await page.route('**/rules/wcag-contrast*', async (route) => {
      const res = await route.fetch();
      const doc = await res.json();
      await route.fulfill({ response: res, json: { ...doc, body: 'WCAG\n\n* item\n\n|a|b|\n|-|-|\n|1|2|' } });
    });
    await page.goto('/policy/rules');
    await page.evaluate(() => localStorage.setItem('ax-web-policy-body-view', 'markdown'));
    await page.goto('/policy/rules?id=wcag-contrast');
    const other = page.getByRole('complementary', { name: 'wcag-contrast' });
    const otherSource = other.locator('textarea.w-md-editor-text-input');
    const original = await otherSource.inputValue();
    await page.evaluate(() => localStorage.setItem('ax-web-policy-body-view', 'wysiwyg'));
    await page.goto('/policy/rules?id=english-only');
    await expect(rich(page)).toBeVisible();
    await page.evaluate(() => {
      window.history.pushState(null, '', '/policy/rules?id=wcag-contrast');
      window.dispatchEvent(new PopStateEvent('popstate'));
    });
    await expect(other.locator('.wysiwyg-content')).toContainText('WCAG');
    await page.waitForTimeout(500);
    await other.getByRole('group', { name: 'Body view' }).getByRole('button', { name: 'Markdown', exact: true }).click();
    await expect(otherSource).toHaveValue(original);
  });

  test('W5 a restored revision replaces the WYSIWYG body without a rewrite', async ({ page }) => {
    const restoredBody = 'Restored\n\n* item\n\n|a|b|\n|-|-|\n|1|2|';
    let restored = false;
    await page.route(/\/api\/policy\/rules\/english-only(\?.*)?$/, async (route) => {
      const res = await route.fetch();
      const doc = await res.json();
      await route.fulfill({ response: res, json: restored ? { ...doc, body: restoredBody } : doc });
    });
    await page.route(/\/api\/policy\/rules\/english-only\/revisions(\?.*)?$/, (route) =>
      route.fulfill({ json: { revisions: [{ id: 1, createdAt: '2026-09-01T10:00:00Z', source: 'save', contentHash: 'abcdef1234' }] } }),
    );
    await page.route(/\/api\/policy\/rules\/english-only\/revisions\/1\/restore/, (route) => {
      restored = true;
      return route.fulfill({ json: { ok: true } });
    });
    await openRule(page, 'wysiwyg');
    await expect(rich(page)).toBeVisible();
    await blade(page).getByRole('button', { name: 'History', exact: true }).click();
    await page.getByRole('button', { name: 'Restore', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect(rich(page)).toContainText('Restored');
    await page.waitForTimeout(500);
    await viewBtn(page, 'Markdown').click();
    await expect(source(page)).toHaveValue(restoredBody);
  });

  test('W2/W4 a real edit saves Markdown and keeps links, comments, tables, code and tasks', async ({ page }) => {
    await openRule(page, 'markdown');
    await source(page).fill(FIXTURE);
    await viewBtn(page, 'WYSIWYG').click();
    await expect(rich(page).locator('h1')).toHaveText('Title');
    await expect(rich(page).locator('[data-wikilink="wcag-contrast"]')).toBeVisible();
    await rich(page).locator('p', { hasText: /^end$/ }).click();
    await page.keyboard.press('End');
    await page.keyboard.type(' now');
    await viewBtn(page, 'Markdown').click();
    const md = await source(page).inputValue();
    for (const piece of [
      '# Title',
      '[[wcag-contrast]]',
      '[[global/rules/x|alias]]',
      '<!-- keep me -->',
      '```rust\nfn main() {}\n```',
      '- [ ] todo',
      '- [x] done',
      'end now',
    ]) {
      expect(md, piece).toContain(piece);
    }
    expect(md).toMatch(/\|\s*a\s*\|\s*b\s*\|\n\|\s*-+\s*\|\s*-+\s*\|\n\|\s*1\s*\|\s*2\s*\|/);
  });

  test('W2 toolbar bold writes ** in Markdown', async ({ page }) => {
    await openRule(page, 'markdown');
    await source(page).fill('plain word');
    await viewBtn(page, 'WYSIWYG').click();
    await rich(page).locator('p').click();
    await page.keyboard.press('End');
    await page.keyboard.press('Shift+Home');
    await blade(page).getByRole('button', { name: 'Bold (⌘B)' }).click();
    await viewBtn(page, 'Markdown').click();
    await expect(source(page)).toHaveValue('**plain word**');
  });

  test('W2 Cmd+I writes * in Markdown', async ({ page }) => {
    await openRule(page, 'markdown');
    await source(page).fill('plain word');
    await viewBtn(page, 'WYSIWYG').click();
    await rich(page).locator('p').click();
    await page.keyboard.press('ControlOrMeta+a');
    await page.keyboard.press('ControlOrMeta+i');
    await expect(rich(page).locator('em')).toHaveText('plain word');
    await viewBtn(page, 'Markdown').click();
    await expect(source(page)).toHaveValue('*plain word*');
  });

  test('W7 [[ picker in the Markdown view inserts a link', async ({ page }) => {
    await openRule(page, 'markdown');
    await source(page).fill('');
    await source(page).pressSequentially('see [[wcag');
    const list = blade(page).getByRole('listbox', { name: 'Link to' });
    await expect(list.getByRole('option').first()).toContainText('wcag-contrast');
    await expect(list.getByRole('option', { name: /english-only/ })).toHaveCount(0);
    const firstId = await list.getByRole('option').first().getAttribute('id');
    await expect(source(page)).toHaveAttribute('aria-activedescendant', firstId!);
    await expect(source(page)).toHaveAttribute('aria-controls', (await list.getAttribute('id'))!);
    await page.keyboard.press('Enter');
    await expect(source(page)).toHaveValue('see [[wcag-contrast]]');
    await expect(list).toHaveCount(0);
  });

  test('W7 Esc closes the Markdown picker without inserting', async ({ page }) => {
    await openRule(page, 'markdown');
    await source(page).fill('');
    await source(page).pressSequentially('[[wc');
    await expect(blade(page).getByRole('listbox', { name: 'Link to' })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(blade(page).getByRole('listbox', { name: 'Link to' })).toHaveCount(0);
    await expect(source(page)).toHaveValue('[[wc');
  });

  test('W7 [[ picker in the WYSIWYG view inserts a link node', async ({ page }) => {
    await openRule(page, 'markdown');
    await source(page).fill('x');
    await viewBtn(page, 'WYSIWYG').click();
    await rich(page).locator('p').click();
    await page.keyboard.press('End');
    await page.keyboard.type(' [[wcag');
    const list = page.getByRole('listbox', { name: 'Link to' });
    await expect(list.getByRole('option').first()).toContainText('wcag-contrast');
    await page.keyboard.press('Enter');
    await expect(rich(page).locator('[data-wikilink="wcag-contrast"]')).toBeVisible();
    await viewBtn(page, 'Markdown').click();
    await expect(source(page)).toHaveValue(/x \[\[wcag-contrast\]\]/);
  });

  test('W7 a failed load says Links unavailable', async ({ page }) => {
    await page.route('**/api/links/graph', (r) => r.fulfill({ status: 500, body: 'no' }));
    await openRule(page, 'markdown');
    await source(page).fill('');
    await source(page).pressSequentially('[[a');
    await expect(blade(page).getByText('Links unavailable')).toBeVisible();
  });
});
