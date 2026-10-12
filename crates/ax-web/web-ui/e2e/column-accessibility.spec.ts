import {test,expect} from '@playwright/test';
const policy={id:'responsive-row',name:'responsive-row',level:'INFO',scope:'project',body:'A readable description of a rule or skill.',enabled:true,status:'approved',alwaysApply:true,priority:1,tags:['graph','workflow'],globs:[],triggers:[],origin:'project',sourcePath:'rules/responsive-row.mdc',effectiveStorage:'files'};
test.beforeEach(async({page})=>{
 await page.route('**/api/**',route=>{
  const p=new URL(route.request().url()).pathname;
  const json=p==='/api/policy/rules'?{rules:[policy]}:p==='/api/policy/skills'?{skills:[policy]}:p==='/api/memory'?{memories:[{id:'responsive-memory',kind:'note',title:'Responsive memory',body:'Keep useful context visible.',tags:['graph','workflow'],files:[],confidence:1,source:'manual',enabled:true,created_at:1791800000000,updated_at:1791800000000}],total:1}:p==='/api/ship/config'?{config:{ship:{},ui:{}},sonar:{}}:p==='/api/stats'?{project_name:'Responsive UI',node_count:0,edge_count:0,file_count:0,languages:[],db_size_bytes:0}:{};
  return route.fulfill({json});
 });
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
