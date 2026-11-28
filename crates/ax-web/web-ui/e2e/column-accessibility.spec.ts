import {test,expect} from '@playwright/test';
import {memoryRow,mockApi,policyRow,type Reply} from './fixtures';
const policy=policyRow();
/** Rules, skills and memory lists with one row each, plus the endpoints their pages read. */
function listApi(overrides:Record<string,Reply>={}):Record<string,Reply>{
 return {
  '/api/policy/rules':{rules:[policy]},
  '/api/policy/skills':{skills:[policy]},
  '/api/policy/settings':{storage:'files'},
  '/api/policy/rules/catalog/audits':[],
  '/api/policy/skills/catalog/audits':[],
  '/api/memory':{memories:[memoryRow()],total:1},
  '/api/links':{outgoing:[],backlinks:[]},
  ...overrides,
 };
}
test.beforeEach(async({page})=>{
 await mockApi(page,listApi());
});

test('column separators keep visible focus in system high contrast',async({page})=>{
 await page.emulateMedia({forcedColors:'active'});await page.setViewportSize({width:1366,height:900});await page.goto('/policy/rules');
 const handle=page.getByRole('separator',{name:'Resize Name column'}).first();await handle.focus();await handle.press('ArrowRight');await expect(handle).toHaveCSS('outline-style','solid');await expect(handle).toHaveCSS('outline-width','1px');
});

test('rightmost column filters stay inside the list viewport',async({page})=>{
 await page.setViewportSize({width:1366,height:900});await page.goto('/policy/rules');
 const list=page.locator('.portal-policy-list').first();const header=list.locator('.portal-data-header');await header.locator('.column-filter').last().locator('summary').click();
 const panel=header.locator('.column-filter-panel').last();await expect(panel).toBeVisible();const field=(await panel.boundingBox())!;const bounds=(await list.boundingBox())!;expect(field.x).toBeGreaterThanOrEqual(bounds.x);expect(field.x+field.width).toBeLessThanOrEqual(bounds.x+bounds.width+1);
});
