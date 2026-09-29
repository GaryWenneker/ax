import { expect, test, type Page } from '@playwright/test';

const node = (kind: string, label: string, tags: string[]) => ({
  key: `${kind}:project:${label}`,
  kind,
  label,
  origin: 'project',
  target: kind === 'memory' ? `memories/${label}` : label,
  tags,
});

const NODES = [
  ...Array.from({ length: 40 }, (_, n) => node('rule', `rule-${String(n).padStart(2, '0')}`, ['misc'])),
  node('rule', 'english-only', ['language']),
  node('skill', 'azdo-pr-review', ['azure', 'azdo', 'pr']),
  node('skill', 'deploy', ['release']),
  node('memory', 'Azure CLI login fix', ['azcli']),
];

const blade = (page: Page) => page.getByRole('complementary', { name: 'english-only' });
const source = (page: Page) => blade(page).locator('textarea.w-md-editor-text-input');
const rich = (page: Page) => blade(page).locator('.wysiwyg-content');
const viewBtn = (page: Page, name: string) =>
  blade(page).getByRole('group', { name: 'Body view' }).getByRole('button', { name, exact: true });
const linkBtn = (page: Page) => blade(page).getByRole('toolbar', { name: 'Formatting' }).getByRole('button', { name: /^Link/ });
const picker = (page: Page) => page.getByRole('dialog', { name: 'Insert link' });
const list = (page: Page) => page.getByRole('listbox', { name: 'Link to' });
const chip = (page: Page, name: string) =>
  page.getByRole('group', { name: 'Filter links' }).getByRole('button', { name: new RegExp(`^${name}`) });

async function openWysiwyg(page: Page, body: string) {
  await page.route('**/api/links/graph', (r) => r.fulfill({ json: { nodes: NODES, edges: [] } }));
  await page.goto('/policy/rules');
  await page.evaluate(() => localStorage.setItem('ax-web-policy-body-view', 'markdown'));
  await page.goto('/policy/rules?id=english-only');
  await expect(blade(page)).toBeVisible();
  await source(page).fill(body);
  await viewBtn(page, 'WYSIWYG').click();
  await expect(rich(page)).toBeVisible();
}

test.describe('Link picker search', () => {
  test.skip(({ isMobile }) => isMobile, 'desktop blade');

  test('B1 the toolbar link button opens the picker with a focused search box', async ({ page }) => {
    await openWysiwyg(page, 'x');
    await rich(page).locator('p').click();
    await linkBtn(page).click();
    await expect(picker(page)).toBeVisible();
    await expect(picker(page).getByRole('searchbox')).toBeFocused();
  });

  test('B1 the picker opens right under the link button', async ({ page }) => {
    await openWysiwyg(page, 'x');
    await rich(page).locator('p').click();
    await linkBtn(page).click();
    const btn = (await linkBtn(page).boundingBox())!;
    const box = (await picker(page).boundingBox())!;
    expect(Math.abs(box.y - (btn.y + btn.height))).toBeLessThan(12);
    expect(Math.abs(box.x - btn.x)).toBeLessThan(12);
  });

  test('B1 Cmd/Ctrl+K opens the same picker', async ({ page }) => {
    await openWysiwyg(page, 'x');
    await rich(page).locator('p').click();
    await page.keyboard.press('ControlOrMeta+k');
    await expect(picker(page).getByRole('searchbox')).toBeFocused();
  });

  test('P1 an empty query shows rules, skills and memory with group counts', async ({ page }) => {
    await openWysiwyg(page, 'x');
    await rich(page).locator('p').click();
    await linkBtn(page).click();
    await expect(list(page).getByRole('option', { name: /azdo-pr-review/ })).toHaveCount(1);
    await expect(list(page).getByRole('option', { name: /Azure CLI login fix/ })).toHaveCount(1);
    await expect(list(page).getByText('Rules (40)')).toBeVisible();
    await expect(list(page).getByText('Skills (2)')).toBeVisible();
    await expect(list(page).getByText('Memory (1)')).toBeVisible();
  });

  test('B2 picking with text selected links that text', async ({ page }) => {
    await openWysiwyg(page, 'see the review skill');
    await rich(page).locator('p').click();
    await page.keyboard.press('End');
    for (let i = 0; i < 'the review skill'.length; i++) await page.keyboard.press('Shift+ArrowLeft');
    await linkBtn(page).click();
    await picker(page).getByRole('searchbox').fill('azdo');
    await page.keyboard.press('Enter');
    await expect(picker(page)).toHaveCount(0);
    await viewBtn(page, 'Markdown').click();
    await expect(source(page)).toHaveValue('see [[azdo-pr-review|the review skill]]');
  });

  test('B2 picking without a selection inserts the link at the cursor', async ({ page }) => {
    await openWysiwyg(page, 'x');
    await rich(page).locator('p').click();
    await page.keyboard.press('End');
    await linkBtn(page).click();
    await picker(page).getByRole('searchbox').fill('deploy');
    await list(page).getByRole('option', { name: /deploy/ }).click();
    await viewBtn(page, 'Markdown').click();
    await expect(source(page)).toHaveValue('x[[deploy]]');
  });

  test('B3 a URL query offers a web link on the selection', async ({ page }) => {
    await openWysiwyg(page, 'docs');
    await rich(page).locator('p').click();
    await page.keyboard.press('ControlOrMeta+a');
    await linkBtn(page).click();
    await picker(page).getByRole('searchbox').fill('https://example.com/a');
    await expect(list(page).getByRole('option').first()).toContainText('Link to URL');
    await page.keyboard.press('Enter');
    await viewBtn(page, 'Markdown').click();
    await expect(source(page)).toHaveValue('[docs](https://example.com/a)');
  });

  test('B4 Escape closes the picker and gives focus back to the editor', async ({ page }) => {
    await openWysiwyg(page, 'x');
    await rich(page).locator('p').click();
    await linkBtn(page).click();
    await expect(picker(page)).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(picker(page)).toHaveCount(0);
    await expect(rich(page)).toBeFocused();
  });

  test('B4 a click outside closes the picker', async ({ page }) => {
    await openWysiwyg(page, 'x');
    await rich(page).locator('p').click();
    await linkBtn(page).click();
    await expect(picker(page)).toBeVisible();
    await blade(page).getByRole('toolbar', { name: 'Formatting' }).click({ position: { x: 3, y: 3 } });
    await expect(picker(page)).toHaveCount(0);
  });

  test('P2 a filter chip narrows the list to one kind', async ({ page }) => {
    await openWysiwyg(page, 'x');
    await rich(page).locator('p').click();
    await linkBtn(page).click();
    await chip(page, 'Skills').click();
    await expect(chip(page, 'Skills')).toHaveAttribute('aria-pressed', 'true');
    await expect(list(page).getByRole('option')).toHaveCount(2);
    await expect(picker(page).getByRole('searchbox')).toBeFocused();
  });

  test('P2 the filter starts at All each time the picker opens', async ({ page }) => {
    await openWysiwyg(page, 'x');
    await rich(page).locator('p').click();
    await linkBtn(page).click();
    await chip(page, 'Skills').click();
    await page.keyboard.press('Escape');
    await linkBtn(page).click();
    await expect(chip(page, 'All')).toHaveAttribute('aria-pressed', 'true');
  });

  test('S2 #tag searches tags and shows the matching tag', async ({ page }) => {
    await openWysiwyg(page, 'x');
    await rich(page).locator('p').click();
    await linkBtn(page).click();
    await picker(page).getByRole('searchbox').fill('#az');
    await expect(list(page).getByRole('option')).toHaveCount(2);
    await expect(list(page).getByRole('option', { name: /azdo-pr-review/ }).locator('.link-picker-tag--hit').first()).toHaveText('azure');
  });

  test('S3 the [[ picker accepts the kind prefix and shows every kind', async ({ page }) => {
    await openWysiwyg(page, 'x');
    await rich(page).locator('p').click();
    await page.keyboard.press('End');
    await page.keyboard.type(' [[m:');
    await expect(list(page).getByRole('option')).toHaveCount(1);
    await expect(list(page).getByRole('option').first()).toContainText('Azure CLI login fix');
  });

  test('P1 the Markdown [[ picker shows skills and memory on an empty query', async ({ page }) => {
    await openWysiwyg(page, '');
    await viewBtn(page, 'Markdown').click();
    await source(page).pressSequentially('[[');
    await expect(list(page).getByRole('option', { name: /azdo-pr-review/ })).toHaveCount(1);
    await expect(list(page).getByRole('option', { name: /Azure CLI login fix/ })).toHaveCount(1);
  });
});
