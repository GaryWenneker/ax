import {expect, test} from '@playwright/test';
import { chooseSelect } from './chooseSelect';
import {screenshotResponse} from '../../../../site/scripts/screenshot-fixtures.mjs';

test.beforeEach(async ({page}) => {
  await page.addInitScript(() => localStorage.setItem('ax-theme', 'azure-dark'));
  if (process.env.AX_SSE_FIXTURE_URL) {
    const response = screenshotResponse('http://fixture/api/usage/mcp-trace/events');
    const lines = JSON.parse(response.body.match(/data: (.+)/)![1]).slice(0,9);
    lines.push('2026-10-11T12:04:00Z [ax] workspace sync ok files=12 duration_ms=140 tool=workspace');
    const configured = await page.request.post(`${process.env.AX_SSE_FIXTURE_URL}/configure`,{data:{lines}});
    expect(configured.ok()).toBe(true);
  }
});

test('mobile settings uses a section sheet and reaches every section', async ({page, isMobile}) => {
  test.skip(!isMobile, 'Mobile sheet; desktop navigation has separate coverage.');
  await page.goto('/settings');
  const sections=['appearance','command-center','agents','remote','plugins','team','clients','nodes','replication','sharing','policy-sharing','vault','export','embeddings'];
  for (const section of sections) {
    await page.getByRole('button', {name: 'Choose settings section'}).click();
    const sheet=page.getByRole('dialog', {name: 'Settings sections'});
    await expect(sheet).toBeVisible();
    await sheet.locator(`[aria-controls="settings-panel-${section}"]`).click();
    await expect(sheet).toHaveCount(0);
    await expect(page.locator(`#settings-panel-${section}`)).toBeVisible();
    await expect(page.getByRole('button', {name: 'Choose settings section'})).toBeFocused();
  }
});

test('mobile page filters open in a sheet and preserve filtering after dismissal', async ({page, isMobile}) => {
  test.skip(!isMobile);
  await page.goto('/nodes');
  await page.getByRole('button', {name: 'Filters', exact:true}).click();
  const sheet=page.getByRole('dialog', {name: 'Filters', exact:true});
  await sheet.getByPlaceholder('Search symbols…').fill('SettingsPage');
  await sheet.getByRole('button', {name: 'Done', exact:true}).click();
  await expect(sheet).toHaveCount(0);
  await expect(page.locator('.page-item')).toHaveCount(1);
  await expect(page.locator('.page-item')).toContainText('SettingsPage');
  await page.getByRole('button', {name: 'Filters', exact:true}).click();
  await expect(sheet.getByPlaceholder('Search symbols…')).toHaveValue('SettingsPage');
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', {name:'Filters', exact:true})).toBeFocused();
});

test('mobile column controls sort and filter without widening the list', async ({page, isMobile}) => {
  test.skip(!isMobile);
  await page.goto('/nodes');
  await page.getByRole('button', {name:'Sort and filter', exact:true}).click();
  const sheet=page.getByRole('dialog', {name:'Sort and filter', exact:true});
  await sheet.getByRole('button', {name:'Sort Name', exact:true}).click();
  await sheet.getByLabel('Filter Name', {exact:true}).fill('GraphPage');
  await sheet.getByRole('button', {name:'Done', exact:true}).click();
  await expect(page.locator('.page-item')).toHaveCount(1);
  await expect(page.locator('.page-item')).toContainText('GraphPage');
  const row=await page.locator('.page-item').boundingBox();
  expect(row!.width).toBeLessThanOrEqual(page.viewportSize()!.width);
});

test('mobile detail is a viewport dialog and returns focus to the selected row', async ({page, isMobile}) => {
  test.skip(!isMobile);
  for (const path of ['/nodes','/memory','/logging','/policy/rules','/policy/skills']) {
    await page.goto(path);
    const row=page.locator('.page-item').first();
    await expect(row).toBeVisible();
    await row.click();
    const detail=page.locator('[data-mobile-detail="true"]');
    await expect(detail).toHaveAttribute('role','dialog');
    await expect(detail).toHaveAttribute('aria-modal','true');
    const box=await detail.boundingBox();
    expect(box!.x).toBe(0);
    expect(box!.width).toBe(page.viewportSize()!.width);
    expect(box!.y).toBe(0);
    expect(box!.height).toBe(page.viewportSize()!.height);
    await expect(page.locator('.statusbar')).toHaveCSS('visibility','hidden');
    await page.keyboard.press('Escape');
    await expect(detail).toHaveCount(0);
    await expect(row).toBeFocused();
  }
});

test('mobile help opens a sheet and row actions have touch targets', async ({page,isMobile}) => {
  test.skip(!isMobile);
  await page.goto('/memory');
  const help=page.getByRole('button',{name:'About New memory',exact:true});
  await help.click();
  const sheet=page.getByRole('dialog',{name:'About New memory',exact:true});
  await expect(sheet).toContainText('similar existing memories');
  await page.keyboard.press('Escape');
  await expect(help).toBeFocused();
  await page.goto('/policy/rules');
  const actions=page.locator('.policy-calm-row').first().locator('.calm-row-action');
  await expect(actions.first()).toBeVisible();
  expect(await actions.count()).toBeGreaterThan(1);
  for(const action of await actions.all()) {
    const box=await action.boundingBox();
    expect(box!.width).toBeGreaterThanOrEqual(44);
    expect(box!.height).toBeGreaterThanOrEqual(44);
    await expect(action).toHaveCSS('opacity','1');
  }
});

test('mobile navigation is hidden from focus until opened and closes with Escape', async ({page,isMobile}) => {
  test.skip(!isMobile);
  await page.goto('/stats');
  await expect(page.locator('.sidebar')).toHaveAttribute('inert','');
  const trigger=page.getByRole('button',{name:'Toggle menu'});
  await trigger.click();
  await expect(page.locator('.sidebar')).not.toHaveAttribute('inert','');
  await page.keyboard.press('Escape');
  await expect(page.locator('.sidebar')).toHaveAttribute('inert','');
  await expect(trigger).toBeFocused();
});

test('mobile logging filters, quality and status use dismissible overlays', async ({page,isMobile}) => {
  test.skip(!isMobile);
  await page.goto('/logging');
  await expect(page.locator('.page-item')).toHaveCount(10);
  await page.getByRole('button',{name:'Show filters',exact:true}).click();
  const filters=page.getByRole('dialog',{name:'Logging filters'});
  await filters.getByLabel('Search log lines', {exact:true}).fill('workspace sync ok');
  await filters.getByRole('button',{name:'Done',exact:true}).click();
  await expect(page.locator('.page-item')).toHaveCount(1);
  await page.getByRole('button',{name:'Open MCP quality'}).click();
  const quality=page.getByRole('dialog',{name:'MCP quality metrics'});
  await expect(quality).toBeVisible();
  expect((await quality.boundingBox())!.width).toBe(page.viewportSize()!.width);
  await page.keyboard.press('Escape');
  await expect(quality).toHaveCount(0);
  await page.locator('.status-activity').click();
  const activity=page.getByRole('dialog',{name:'Live activity'});
  await expect(activity).toBeVisible();
  await activity.getByRole('button',{name:'Done',exact:true}).click();
  await expect(page.locator('.status-activity')).toBeFocused();
});

test('mobile file preview, source and graph settings remain reachable', async ({page,isMobile}) => {
  test.skip(!isMobile);
  await page.goto('/files');
  await page.locator('.file-tree-row--folder').first().click();
  await page.locator('.file-tree-row--file').first().click();
  const preview=page.getByRole('dialog').filter({has:page.locator('.files-preview-close')});
  await expect(preview).toBeVisible();
  expect((await preview.boundingBox())!.width).toBe(page.viewportSize()!.width);
  await preview.getByRole('tab',{name:'Source'}).click();
  await expect(preview).toContainText('Synthetic source');
  await page.keyboard.press('Escape');
  await expect(preview).toHaveCount(0);
  await page.goto('/graph');
  await page.getByRole('button',{name:'Graph controls',exact:true}).click();
  const controls=page.getByRole('dialog',{name:'Graph controls',exact:true});
  await expect(controls.getByPlaceholder('Search nodes…')).toBeVisible();
  await controls.getByRole('button',{name:'Done',exact:true}).click();
  await page.getByRole('button',{name:'Graph settings',exact:true}).click();
  const settings=page.getByRole('dialog',{name:'Graph settings',exact:true});
  await expect(settings).toHaveAttribute('aria-modal','true');
  await settings.getByPlaceholder('Search files…').fill('module');
  await page.keyboard.press('Escape');
  await expect(settings).toHaveCount(0);
});

test('mobile forms fit small screens, contain focus and keep footer actions visible', async ({page,isMobile}) => {
  test.skip(!isMobile);
  await page.setViewportSize({width:320,height:568});
  await page.goto('/memory');
  await page.getByRole('button',{name:'New memory',exact:true}).click();
  const form=page.getByRole('dialog',{name:'New memory',exact:true});
  await expect(form).toBeVisible();
  const box=await form.boundingBox();
  expect(box!.x).toBeGreaterThanOrEqual(0);expect(box!.width).toBeLessThanOrEqual(320);
  const cancel=form.getByRole('button',{name:'Cancel',exact:true});
  const footer=await cancel.boundingBox();
  expect(footer!.y+footer!.height).toBeLessThanOrEqual(568);
  await expect(form.getByRole('button',{name:'Close',exact:true})).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(cancel).toBeFocused();
  await form.getByPlaceholder('The decision, fix, or convention — and why.').fill('A mobile form draft.');
  await expect(form.getByRole('button',{name:'Save memory',exact:true})).toBeEnabled();
  await cancel.click();
  await expect(form).toHaveCount(0);
  await expect(page.getByRole('button',{name:'New memory',exact:true})).toBeFocused();
});

test('mobile project picker and sheet backdrop dismiss without triggering the page', async ({page,isMobile}) => {
  test.skip(!isMobile);
  await page.goto('/stats');
  await page.locator('.status-project').click();
  const picker=page.getByRole('dialog',{name:'Browse projects'});
  await expect(picker).toBeVisible();
  await expect(picker.locator('.project-browser-search')).not.toBeFocused();
  await page.keyboard.press('Escape');
  await expect(picker).toHaveCount(0);
  await expect(page.locator('.status-project')).toBeFocused();
  await page.goto('/nodes');
  await page.getByRole('button',{name:'Filters',exact:true}).click();
  await page.locator('.ax-modal-overlay').click({position:{x:4,y:4}});
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.locator('.page-item')).toHaveCount(6);
});

test('mobile review and period filters use sheets and comparisons remain readable', async ({page,isMobile}) => {
  test.skip(!isMobile);test.setTimeout(8000);
  await page.goto('/savings');
  await page.getByRole('button',{name:'Period',exact:true}).click();
  const period=page.getByRole('dialog',{name:'Time period',exact:true});
  await chooseSelect(period.getByLabel('Time period',{exact:true}), 'month');
  await period.getByRole('button',{name:'Done',exact:true}).click();
  await expect(page.getByRole('heading',{name:'Context savings',exact:true})).toBeVisible();
  await page.goto('/policy/review');
  await page.getByRole('button',{name:'Filters',exact:true}).click();
  const filters=page.getByRole('dialog',{name:'Filters',exact:true});
  await chooseSelect(filters.getByLabel('Review kind',{exact:true}), 'skill');
  await filters.getByRole('button',{name:'Done',exact:true}).click();
  await expect(page.getByText('No matching items',{exact:true})).toBeVisible();
  await page.getByRole('button',{name:'Filters',exact:true}).click();
  await chooseSelect(filters.getByLabel('Review kind',{exact:true}), 'all');
  await filters.getByRole('button',{name:'Done',exact:true}).click();
  await page.getByRole('button',{name:'Inspect content and source',exact:true}).click();
  const comparison=page.getByRole('dialog',{name:'Inspect content and source',exact:true});
  await expect(comparison.getByRole('heading',{name:'Current',exact:true})).toBeVisible();
  await expect(comparison.getByRole('heading',{name:'Incoming',exact:true})).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(comparison).toHaveCount(0);
});

test('mobile editors use full-width source and preview tabs', async ({page,isMobile}) => {
  test.skip(!isMobile);test.setTimeout(8000);
  await page.goto('/policy/rules/edit');
  await page.getByRole('tab',{name:'Markdown',exact:true}).click();
  const editor=page.locator('.md-editor-split-edit').first();
  await expect(editor).toBeVisible();
  expect((await editor.boundingBox())!.width).toBeGreaterThan(page.viewportSize()!.width*.8);
  await editor.locator('textarea').fill('Mobile editor content');
  await page.getByRole('tab',{name:'Preview',exact:true}).click();
  await expect(page.locator('.md-editor-split-preview')).toContainText('Mobile editor content');
  await expect(editor).toHaveCount(0);
});
