import {test,expect} from '@playwright/test';
const policy={id:'responsive-row',name:'responsive-row',level:'INFO',scope:'project',body:'A readable description of a rule or skill.',enabled:true,status:'approved',alwaysApply:true,priority:1,tags:['graph','workflow'],globs:[],triggers:[],origin:'project',sourcePath:'rules/responsive-row.mdc',effectiveStorage:'files'};
test.beforeEach(async({page})=>{
 await page.route('**/api/**',route=>{
  const p=new URL(route.request().url()).pathname;
  const json=p==='/api/links'?{outgoing:[],backlinks:[]}:p==='/api/policy/rules/Alpha'?{frontmatter:{...policy,id:'Alpha'},body:policy.body,raw:policy.body,sourcePath:'rules/Alpha.mdc'}:p==='/api/policy/rules'?{rules:[{...policy,id:"Zulu",name:"Zulu",priority:10},{...policy,id:"Alpha",name:"Alpha",priority:2}]}:p==='/api/policy/skills'?{skills:[policy]}:p==='/api/memory'?{memories:[{id:'responsive-memory',kind:'note',title:'Responsive memory',body:'Keep useful context visible.',tags:['graph','workflow'],files:[],confidence:1,source:'manual',enabled:true,created_at:1791800000000,updated_at:1791800000000}],total:1}:p==='/api/ship/config'?{config:{ship:{},ui:{}},sonar:{}}:p==='/api/stats'?{project_name:'Responsive UI',node_count:0,edge_count:0,file_count:0,languages:[],db_size_bytes:0}:{};
  return route.fulfill({json});
 });
});

test('column drag keyboard resizing persists and resets independently',async({page})=>{
 await page.setViewportSize({width:1366,height:900});await page.goto('/policy/rules');
 const handle=page.getByRole('separator',{name:'Resize Name column'}).first();await expect(handle).toBeVisible();
 const header=page.locator('.portal-data-header').first();
 const before=await header.locator('.column-heading').evaluateAll(es=>es.map(e=>e.getBoundingClientRect().width));
 const rect=(await handle.boundingBox())!;await page.mouse.move(rect.x+rect.width/2,rect.y+rect.height/2);await page.mouse.down();await page.mouse.move(rect.x+rect.width/2+60,rect.y+rect.height/2);await page.mouse.up();
 const resized=await header.locator('.column-heading').evaluateAll(es=>es.map(e=>e.getBoundingClientRect().width));
 expect(resized[0]-before[0]).toBeGreaterThan(50);expect(Math.abs(resized[1]-before[1])).toBeLessThan(2);
 await page.reload();expect((await header.locator('.column-heading').first().boundingBox())!.width).toBeCloseTo(resized[0],0);
 await handle.focus();await handle.press('ArrowRight');expect((await header.locator('.column-heading').first().boundingBox())!.width).toBeGreaterThan(resized[0]+8);
 await page.locator('.column-tools').first().locator('summary').click();await page.getByRole('button',{name:'Reset columns',exact:true}).first().click();
 expect((await header.locator('.column-heading').first().boundingBox())!.width).toBeCloseTo(before[0],0);
});

test('headings sort numeric values and column filters preserve row actions',async({page})=>{
 await page.setViewportSize({width:1366,height:900});await page.goto('/policy/rules');
 const names=()=>page.locator('.portal-data-row .portal-data-name > .page-item-title').allTextContents();
 const header=page.locator('.portal-data-header').first();await header.getByRole('button',{name:'Sort Priority',exact:true}).click();
 expect(await names()).toEqual(['Alpha','Zulu']);await header.getByRole('button',{name:'Sort Priority',exact:true}).click();expect(await names()).toEqual(['Zulu','Alpha']);
 await header.locator('summary[aria-label="Filter Name"]').click();await header.getByRole('textbox',{name:'Filter Name',exact:true}).fill('alp');
 expect(await names()).toEqual(['Alpha']);await expect(page.getByRole('button',{name:'Edit Alpha',exact:true})).toBeVisible();
 await page.getByRole('button',{name:'Edit Alpha',exact:true}).click();await expect(page.getByRole('heading',{name:'Edit rule: Alpha',exact:true})).toBeVisible();
});

test('mobile Columns menu offers sort and filters without row overflow',async({page})=>{
 await page.setViewportSize({width:393,height:900});await page.goto('/policy/rules');
 const tools=page.locator('.column-tools').first();await tools.locator('summary').click();await tools.getByRole('button',{name:'Sort Name',exact:true}).click();await tools.getByRole('textbox',{name:'Filter Name',exact:true}).fill('Zulu');
 await expect(page.locator('.portal-data-row')).toHaveCount(1);
 expect(await page.locator('.portal-data-row').evaluate(e=>e.scrollWidth<=e.clientWidth+1)).toBe(true);
});

test('shared data tables expose independent columns and numeric sorting',async({page})=>{
 await page.route('**/api/stats',route=>route.fulfill({json:{project_name:'Column tables',node_count:12,edge_count:0,file_count:0,languages:[{language:'Zulu',count:10},{language:'Alpha',count:2}],db_size_bytes:0}}));
 await page.setViewportSize({width:1366,height:900});await page.goto('/stats');
 const table=page.locator('.column-table').first();await expect(table.locator('tbody tr')).toHaveCount(2);
 await table.getByRole('button',{name:'Sort Nodes',exact:true}).click();expect(await table.locator('tbody tr td:first-child').allTextContents()).toEqual(['Alpha','Zulu']);
 const handle=table.getByRole('separator',{name:'Resize Language column'});await handle.focus();
 const before=await table.locator('th').evaluateAll(es=>es.map(e=>e.getBoundingClientRect().width));await handle.press('ArrowRight');
 const after=await table.locator('th').evaluateAll(es=>es.map(e=>e.getBoundingClientRect().width));expect(after[0]-before[0]).toBeGreaterThan(8);expect(Math.abs(after[1]-before[1])).toBeLessThan(2);
 for(let i=0;i<10;i++) await handle.press('ArrowLeft');
 const shrunk=await table.locator('th').evaluateAll(es=>es.map(e=>e.getBoundingClientRect().width));expect(before[0]-shrunk[0]).toBeGreaterThan(80);expect(Math.abs(shrunk[1]-before[1])).toBeLessThan(2);
});

test('Nodes Search and Unresolved use the shared list columns',async({page})=>{
 const nodes=['Zulu','Alpha'].map((name,i)=>({id:String(i),kind:'function',name,qualified_name:name,file_path:`src/${name}.ts`,language:'typescript',start_line:1,end_line:2,signature:null,is_exported:1}));
 await page.route(/\/api\/nodes\?/,route=>route.fulfill({json:{nodes,total:2}}));
 await page.route(/\/api\/search\?/,route=>route.fulfill({json:{results:nodes.map(n=>({...n,snippet:n.file_path}))}}));
 await page.route(/\/api\/unresolved\?/,route=>route.fulfill({json:{refs:nodes.map((n,i)=>({id:i,from_node_id:n.id,reference_name:n.name,reference_kind:'calls',line:1,col:1,file_path:n.file_path,language:n.language})),total:2}}));
 await page.route('**/api/unresolved/summary',route=>route.fulfill({json:{total:2,by_kind:[],top_names:[]}}));
 await page.setViewportSize({width:1366,height:900});
 for(const path of ['/nodes','/search','/unresolved']){
  await page.goto(path);if(path==='/search') await page.getByPlaceholder('Search symbols, functions, classes…').fill('symbols');
  await expect(page.locator('.item-column-row')).toHaveCount(2);
  const header=page.locator('.item-column-header').first();await header.getByRole('button',{name:'Sort Name',exact:true}).click();
  expect(await page.locator('.item-column-name .page-item-title').allTextContents()).toEqual(['Alpha','Zulu']);
  await page.setViewportSize({width:393,height:900});expect(await page.locator('.item-column-row').first().evaluate(e=>e.scrollWidth<=e.clientWidth+1)).toBe(true);
  await page.setViewportSize({width:1366,height:900});
 }
});

test('review column filters govern the bulk selection and preserve comparisons',async({page})=>{
 await page.route('**/api/policy/review',route=>route.fulfill({json:{items:[{kind:'rule',id:'New',scope:'project',effect:'new',levelOrDescription:'INFO',path:'new.mdc',preview:'New content',incomingContent:'New content',currentContent:null},{kind:'rule',id:'Duplicate',scope:'project',effect:'duplicate',levelOrDescription:'INFO',path:'duplicate.mdc',preview:'Matching content',incomingContent:'Matching content',currentContent:'Matching content'}]}}));
 await page.setViewportSize({width:1366,height:900});await page.goto('/policy/review');
 const header=page.locator('.review-column-list>.column-header');await header.locator('summary[aria-label="Filter Effect"]').click();await header.getByRole('textbox',{name:'Filter Effect',exact:true}).fill('Duplicate');
 await expect(page.locator('.review-item')).toHaveCount(1);await page.getByRole('button',{name:'Select filtered (1)',exact:true}).click();await expect(page.getByText('1 selected in this filter',{exact:true})).toBeVisible();
 await page.getByText('Inspect content and source',{exact:true}).click();await expect(page.locator('.review-comparison pre')).toHaveText(['Matching content','Matching content']);
});

test('Memory enabled filters preserve the legacy default and explicit disabled state',async({page})=>{
 const base={kind:'note',body:'Useful context',tags:[],files:[],confidence:1,source:'manual',created_at:1791800000000,updated_at:1791800000000};
 await page.route(/\/api\/memory(?:\?|$)/,route=>route.fulfill({json:{memories:[{...base,id:'implicit',title:'Default enabled'},{...base,id:'explicit',title:'Explicit enabled',enabled:true},{...base,id:'disabled',title:'Disabled memory',enabled:false}],total:3}}));
 await page.setViewportSize({width:1366,height:900});await page.goto('/memory');await expect(page.locator('.portal-data-row')).toHaveCount(3);
 const header=page.locator('.portal-memory-list .portal-data-header');await header.locator('summary[aria-label="Filter Enabled"]').click();const input=header.getByRole('textbox',{name:'Filter Enabled',exact:true});await input.fill('enabled');
 await expect(page.locator('.portal-data-row')).toHaveCount(2);await expect(page.getByRole('button',{name:'Disable Default enabled',exact:true})).toBeVisible();
 await input.fill('disabled');await expect(page.locator('.portal-data-row')).toHaveCount(1);await expect(page.getByRole('button',{name:'Enable Disabled memory',exact:true})).toBeVisible();
});
