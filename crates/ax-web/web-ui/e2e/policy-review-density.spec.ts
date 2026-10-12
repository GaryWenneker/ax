import { test, expect } from '@playwright/test';
import { ACCENT_COLORS, THEMES, contrastRatioHex } from '../src/lib/themes';

test('retired Quality Gates bookmarks return to the homepage', async ({ page }) => {
  for (const suffix of ['', '?embed=1']) {
    await page.goto('/ship'+suffix);
    await expect(page).toHaveURL(/\/stats$/);
    await expect(page.getByRole('navigation',{name:'Main navigation'})).toHaveCount(1);
    await expect(page.getByRole('heading',{name:'Quality gates',exact:true})).toHaveCount(0);
  }
});

test('accent choices adapt to each mode, persist and preserve header/footer surfaces', async ({ page }) => {
  await page.goto('/settings');
  await page.locator('[aria-controls="settings-panel-appearance"]').click();
  for (const theme of THEMES) {
    await page.getByRole('button',{name:new RegExp(`^${theme.label}( \\(current\\))?$`)}).click();
    const before = await page.locator('.titlebar, .statusbar').evaluateAll(elements=>elements.map(el=>getComputedStyle(el).backgroundColor));
    for (const color of ACCENT_COLORS) {
      await page.getByRole('button',{name:color.id === 'lime' ? 'Lime (default)' : color.label,exact:true}).click();
      await expect(page.getByRole('button',{name:color.id === 'lime' ? 'Lime (default)' : color.label,exact:true})).toHaveAttribute('aria-pressed','true');
      const tokens = await page.locator('html').evaluate(el=>{
        const s=getComputedStyle(el);return {fill:s.getPropertyValue('--accent').trim(),ink:s.getPropertyValue('--accent-on-fill').trim(),text:s.getPropertyValue('--accent-text').trim(),bg:s.getPropertyValue('--bg-panel').trim()};
      });
      expect(contrastRatioHex(tokens.text,tokens.bg)).toBeGreaterThanOrEqual(7);
      expect(contrastRatioHex(tokens.ink,tokens.fill)).toBeGreaterThanOrEqual(4.5);
      expect(await page.locator('.titlebar, .statusbar').evaluateAll(elements=>elements.map(el=>getComputedStyle(el).backgroundColor))).toEqual(before);
    }
    await page.reload();
    await expect(page.getByRole('button',{name:'Rose',exact:true})).toHaveAttribute('aria-pressed','true');
  }
  await page.getByRole('button',{name:'Lime (default)',exact:true}).click();
  if (process.env.AX_ACCESS_SCREENSHOTS && test.info().project.name === 'desktop-chrome') await page.locator('#settings-panel-appearance').screenshot({path:'../../../site/public/screenshots/cc-accent-colors.png'});
});

test('savings values share baselines and headers remain compact across viewport sizes', async ({ page }) => {
  for (const width of [2048, 900, 390]) {
    await page.setViewportSize({width,height:900});
    await page.goto('/savings');
    await expect(page.locator('.sv-hero-stat')).toHaveCount(10);
    const rows = await page.locator('.sv-hero-stat').evaluateAll(cells => cells.map(cell => {
      const box = cell.getBoundingClientRect();
      const value = cell.querySelector('.sv-hero-stat-value')!.getBoundingClientRect();
      return {top:box.top,valueTop:value.top,overflow:cell.scrollWidth > cell.clientWidth + 1};
    }));
    for (const cell of rows) {
      const peers = rows.filter(row => Math.abs(row.top - cell.top) < 1);
      expect(Math.max(...peers.map(row => row.valueTop)) - Math.min(...peers.map(row => row.valueTop))).toBeLessThan(1);
      expect(cell.overflow).toBeFalsy();
    }
    expect(await page.locator('.settings-hero').evaluate(el => getComputedStyle(el).paddingTop)).toBe('6px');
    expect(await page.locator('.sv-page').evaluate(el => el.scrollWidth <= el.clientWidth + 1)).toBeTruthy();
    if (process.env.AX_ACCESS_SCREENSHOTS && width === 2048 && test.info().project.name === 'desktop-chrome') await page.locator('.sv-hero-strip').screenshot({path:'../../../site/public/screenshots/cc-savings-aligned.png'});
  }
});

test('review wraps content and supports scoped bulk decisions with partial failure', async ({ page }) => {
  let items = [
    {kind:'rule',id:'new-rule',effect:'new',currentContent:null},
    {kind:'skill',id:'same',effect:'duplicate',currentContent:'Current skill'},
    {kind:'rule',id:'same',effect:'replacement',currentContent:'Current rule'},
  ].map(item => ({...item,path:'.ax/policy/pending/'+item.id,scope:'project',origin:'Imported policy; peer unknown',status:'pending',preview:'Incoming content',incomingContent:'Incoming content\n'+'long-content-'.repeat(80),levelOrDescription:'A detailed explanation that must remain readable.'}));
  const decisions: {path:string;body:unknown}[] = [];
  await page.route('**/api/**', async route => {
    const url = new URL(route.request().url()); const path = url.pathname;
    if (path.includes('/review/') && route.request().method() === 'POST') {
      decisions.push({path:path+'?'+url.searchParams.toString(),body:route.request().postDataJSON()});
      const id = decodeURIComponent(path.split('/')[4]); const kind = url.searchParams.get('kind');
      if (kind === 'rule' && id === 'same') return route.fulfill({status:409,json:{error:'Policy changed since inspection'}});
      items = items.filter(item => item.kind !== kind || item.id !== id);
      return route.fulfill({json:{ok:true}});
    }
    const data = path === '/api/policy/review' ? {items}
      : path === '/api/stats' ? {project_name:'Review demo',readonly:false,node_count:0,edge_count:0,file_count:0,languages:[],db_size_bytes:0,policy_rules_count:2,policy_skills_count:1,last_indexed_at:1791820800}
      : path === '/api/ship/config' ? {config:{ship:{},ui:{}},sonar:{}}
      : path === '/api/policy/settings' ? {requireReview:true,policySync:true,storage:'database'} : {};
    await route.fulfill({json:data});
  });
  await page.goto('/policy/review');
  await expect(page.getByRole('heading',{name:'Pending changes'})).toBeVisible();
  await expect(page.locator('.review-item')).toHaveCount(3);
  await page.locator('.review-item').last().getByText('Inspect content and source',{exact:true}).click();
  await expect(page.locator('.review-item').last().getByText('Current rule',{exact:true})).toBeVisible();
  await page.locator('.review-item').last().locator('pre').last().scrollIntoViewIfNeeded();
  await expect(page.locator('.review-item').last().locator('pre').last()).toBeInViewport();
  expect(await page.locator('.review-page').evaluate(el => el.scrollWidth <= el.clientWidth + 1)).toBeTruthy();
  if (process.env.AX_ACCESS_SCREENSHOTS && test.info().project.name === 'desktop-chrome') await page.locator('.review-item').last().screenshot({path:'../../../site/public/screenshots/cc-policy-review-detail.png'});
  await page.locator('.review-item').last().getByText('Inspect content and source',{exact:true}).click();
  await page.getByRole('heading',{name:'Review',exact:true}).scrollIntoViewIfNeeded();
  if (process.env.AX_ACCESS_SCREENSHOTS) await page.screenshot({path:`../../../site/public/screenshots/cc-policy-review-${test.info().project.name}.png`});
  await page.getByRole('combobox',{name:'Review kind'}).selectOption('skill');
  await page.getByRole('button',{name:'Approve filtered type',exact:true}).click();
  await page.getByRole('button',{name:'Cancel',exact:true}).click();
  expect(decisions).toHaveLength(0);
  await page.getByRole('button',{name:'Select filtered (1)',exact:true}).click();
  await page.getByRole('button',{name:'Approve selected',exact:true}).click();
  await page.getByRole('button',{name:'Confirm decision',exact:true}).click();
  await expect(page.locator('.review-item')).toHaveCount(0);
  expect(decisions[0].path).toContain('same/approve?kind=skill');
  expect(decisions[0].body).toMatchObject({expectedIncoming:expect.any(String),expectedCurrent:'Current skill'});
  await page.getByRole('combobox',{name:'Review kind'}).selectOption('rule');
  await expect(page.locator('.review-item')).toHaveCount(2);
  await page.getByRole('button',{name:'Select filtered (2)',exact:true}).click();
  await page.getByRole('button',{name:'Approve selected',exact:true}).click();
  await page.getByRole('button',{name:'Confirm decision',exact:true}).click();
  await expect(page.getByText(/1 completed. Remaining items were not processed/)).toBeVisible();
  await expect(page.locator('.review-item')).toHaveCount(1);
  await expect(page.getByRole('checkbox',{name:'Select rule same'})).toBeChecked();
});
