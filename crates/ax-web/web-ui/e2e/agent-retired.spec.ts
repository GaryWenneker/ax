import { test, expect } from '@playwright/test';
for (const path of ['/agent', '/#agent']) {
  test(`retired ${path} opens Stats without Agent navigation`, async ({ page }) => {
    await page.goto(path);
    await expect(page).toHaveURL(/\/stats$/);
    if (test.info().project.name === 'mobile-chrome') await page.getByRole('button', { name: /Toggle menu/i }).click();
    await expect(page.locator('nav').getByRole('button', { name: 'Agent', exact: true })).toHaveCount(0);
    await expect(page.locator('.agent-page')).toHaveCount(0);
    await expect(page.locator('nav').getByRole('button', { name: 'Logging', exact: true })).toBeVisible();
  });
}

test('accent swatches follow Light and Dark and survive reload', async ({ page }) => {
  await page.goto('/settings?section=appearance');
  for (const mode of ['Light', 'Dark']) {
    await page.getByRole('button', { name: new RegExp(`^${mode}`) }).click();
    await expect(page.locator('.accent-chooser')).toContainText(`Accent color · ${mode}`);
    const lime = page.getByRole('button', { name: 'Lime (default)', exact: true });
    await lime.click();
    const expected = mode === 'Light' ? 'rgb(56, 105, 0)' : 'rgb(185, 255, 106)';
    await expect(lime.locator('.accent-dot')).toHaveCSS('background-color', expected);
    await page.reload();
    await expect(page.locator('.accent-chooser')).toContainText(`Accent color · ${mode}`);
    await expect(lime.locator('.accent-dot')).toHaveCSS('background-color', expected);
  }
});

test('Memory starts with compact help and rows near the top', async ({ page }) => {
  await page.route(/\/api\/memory(?:\?|$)/, route => route.fulfill({json:{memories:[{id:'compact-test',kind:'note',title:'Compact memory row',body:'Fixture memory.',tags:[],files:[],confidence:1,source:'manual',enabled:true,created_at:1791800000000,updated_at:1791800000000}],total:1}}));
  await page.goto('/memory');
  await expect(page.locator('.mem-overview-help')).not.toHaveAttribute('open', '');
  const header = page.locator('.portal-data-header');
  await expect(header).toBeVisible();
  const box = await header.boundingBox();
  expect(box?.y).toBeLessThan(test.info().project.name === 'mobile-chrome' ? 430 : 300);
  await page.locator('.mem-overview-help summary').click();
  await expect(page.locator('.mem-overview-help')).toHaveAttribute('open', '');
  await expect(page.locator('.mem-how-grid')).toBeVisible();
  await page.locator('.mem-overview-help summary').click();
  await expect(page.locator('.mem-how-grid')).toBeHidden();
});

test('shared list page headers keep explanations collapsed', async ({ page }) => {
  for (const path of ['/policy/rules','/policy/skills','/nodes','/files','/unresolved','/stats','/search','/graph','/settings','/prices','/policy/review','/policy/sync','/ship']) {
    await page.goto(path);
    const hero=page.locator('.settings-hero').first();
    await expect(hero).toBeVisible();
    const box=await hero.boundingBox();
    expect(box?.height).toBeLessThan(test.info().project.name === 'mobile-chrome' ? 180 : 105);
    const help=hero.locator('.page-header-help');
    if(await help.count()) await expect(help).not.toHaveAttribute('open','');
  }
});

test('Rules and Skills badges share the same width for varied labels', async ({ page }) => {
  const rows = ['CRITICAL','WARNING','INFO'].map((level,i)=>({id:`badge-${i}`,name:`badge-${i}`,level,scope:i===0?'project':i===1?'global':'git',body:'Fixture policy.',enabled:true,status:'approved',alwaysApply:true,priority:i,tags:[],globs:[],triggers:[],origin:'project',sourcePath:`rules/badge-${i}.mdc`,effectiveStorage:'files'}));
  await page.route(/\/api\/policy\/rules(?:\?|$)/,route=>route.fulfill({json:{rules:rows}}));
  await page.route(/\/api\/policy\/skills(?:\?|$)/,route=>route.fulfill({json:{skills:rows}}));
  for(const path of ['/policy/rules','/policy/skills']) {
    await page.goto(path);
    const badges=page.locator('.policy-calm-row .portal-data-cell :is(.page-level-badge,.page-scope-badge)');
    await expect(badges.first()).toBeVisible();
    const widths=await badges.evaluateAll(elements=>elements.map(el=>el.getBoundingClientRect().width));
    expect(widths.length).toBeGreaterThanOrEqual(3);
    for(const width of widths) expect(width).toBe(100);
  }
});
