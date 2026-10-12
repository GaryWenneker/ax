import {test,expect} from '@playwright/test';
import {emptyQualitySnapshot} from '../src/lib/mcpQuality';
const streamFixture=process.env.AX_SSE_FIXTURE_URL;

const lines=[
 '2026-10-12T20:00:00Z [ax-mcp] inbound tool=ax_search args={"query":"Restore readable logging events with useful details"}',
 '2026-10-12T20:00:01Z [ax-mcp] outbound tool=ax_search result={"count":3}',
 '2026-10-12T20:00:02Z [ax-mcp] internal tool=ax_status index refreshed',
];
test.beforeEach(async({page})=>{
 if(streamFixture) {const configured=await page.request.post(`${streamFixture}/configure`,{data:{lines}});expect(configured.ok()).toBe(true);}
 let sent=false;
 await page.route('**/api/**',route=>{
  const path=new URL(route.request().url()).pathname;
  if(path==='/api/usage/mcp-trace/events') {
   if(streamFixture) return route.continue();
   const body=sent?': heartbeat\n\n':`event: batch\ndata: ${JSON.stringify(lines)}\n\n`;sent=true;
   return route.fulfill({contentType:'text/event-stream',body});
  }
  const json=path==='/api/usage/mcp-quality'?{...emptyQualitySnapshot(),projectLabel:'Logging layout',verbosePresent:true,verboseEnabled:true}:path==='/api/usage/mcp-trace/path'?{projectLabel:'Logging layout',projectRoot:'/fixtures/logging',logDay:'2026-10-12',logExists:true}:path==='/api/ship/config'?{config:{ship:{},ui:{}},sonar:{}}:path==='/api/stats'?{project_name:'Logging layout',node_count:0,edge_count:0,file_count:0,languages:[],db_size_bytes:0}:{};
  return route.fulfill({json});
 });
});

test('logging retains stacked left-aligned event details at every panel width',async({page})=>{
 await page.goto('/logging');const rows=page.locator('.mcp-trace-row');await expect(rows).toHaveCount(3);
 await expect(page.locator('.mcp-trace-row--outbound .page-item-title')).toContainText('ax_search');
 for(const width of [2048,1366,900,393]) {
  await page.setViewportSize({width,height:900});
  const row=rows.first();await expect(row).toBeVisible();
  const geometry=await row.evaluate(e=>{
   const body=e.querySelector('.page-item-body')!;
   const title=body.querySelector('.page-item-title-line')!.getBoundingClientRect();
   const sub=body.querySelector('.page-item-sub')!.getBoundingClientRect();
   return {left:Math.abs(title.left-sub.left),gap:sub.top-title.bottom,overflow:e.scrollWidth>e.clientWidth+1};
  });
  expect(geometry.left).toBeLessThanOrEqual(1);expect(geometry.gap).toBeLessThanOrEqual(3);expect(geometry.gap).toBeGreaterThanOrEqual(0);
  expect(geometry.overflow).toBe(false);
  if(process.env.AX_COLUMN_SCREENSHOTS && test.info().project.name==='desktop-chrome' && (width===1366 || width===393)) await page.screenshot({path:`../../../site/public/screenshots/cc-logging-columns${width===393?'-mobile':''}.png`});
 }
 await page.setViewportSize({width:2048,height:900});
 await rows.first().locator('.page-item-title').click();
 await expect(page.locator('.mcp-blade-step').first()).toBeVisible();
 await expect(rows.first()).toHaveClass(/page-item--selected/);
});

test('logging kind filters still show matching events',async({page})=>{
 await page.goto('/logging');await expect(page.locator('.mcp-trace-row')).toHaveCount(3);
 await page.locator('.mcp-trace-row--inbound button.mcp-kind-badge').click();
 await expect(page.locator('.mcp-trace-row')).toHaveCount(1);
 await expect(page.locator('.mcp-trace-row').first()).toHaveClass(/mcp-trace-row--inbound/);
});

test('logging keyboard navigation follows the sorted visible rows',async({page})=>{
 await page.setViewportSize({width:2048,height:900});await page.goto('/logging');await expect(page.locator('.mcp-trace-row')).toHaveCount(3);
 await page.locator('.mcp-trace-calm .column-header').getByRole('button',{name:'Sort Time',exact:true}).click();
 const scroller=page.locator('.mcp-trace-scroller');await scroller.focus();await scroller.press('Home');
 await expect(page.locator('.mcp-trace-row').first()).toHaveAttribute('aria-selected','true');
 await scroller.press('ArrowDown');await expect(page.locator('.mcp-trace-row').nth(1)).toHaveAttribute('aria-selected','true');
});

test('live log arrivals obey active column sort and filters',async({page},info)=>{
 const streamId='live-'+info.project.name;
 let deliveries=0;
 await page.route('**/api/usage/mcp-trace/events',route=>{
  if(streamFixture) return route.continue();
  const body=deliveries++===0?`event: batch\ndata: ${JSON.stringify(lines)}\n\n`:deliveries===2?'event: line\ndata: 2026-10-12T19:59:00Z [ax-mcp] inbound tool=ax_search args={"query":"Earlier arriving event"}\n\n':': heartbeat\n\n';
  return route.fulfill({contentType:'text/event-stream',body});
 });
 await page.setViewportSize({width:2048,height:900});await page.goto('/logging');await expect(page.locator('.mcp-trace-row')).toHaveCount(3);
 const header=page.locator('.mcp-trace-calm .column-header');await header.getByRole('button',{name:'Sort Time',exact:true}).click();
 await header.locator('summary[aria-label="Filter Context"]').click();await header.getByRole('textbox',{name:'Filter Context',exact:true}).fill('ax_search');await expect(page.locator('.mcp-trace-row')).toHaveCount(2);
 if(streamFixture) {const emitted=await page.request.get(`${streamFixture}/emit?id=${streamId}&line=${encodeURIComponent('2026-10-12T19:59:00Z [ax-mcp] inbound tool=ax_search args={"query":"Earlier arriving event"}')}`);expect(emitted.ok()).toBe(true);}
 await expect(page.locator('.mcp-trace-row')).toHaveCount(3);await expect(page.locator('.mcp-trace-row').first()).toContainText('Earlier arriving event');
});

test('logging kind chips fit their compact context labels',async({page})=>{
 await page.setViewportSize({width:1366,height:900});await page.goto('/logging');await expect(page.locator('.mcp-trace-row')).toHaveCount(3);
 const widths=await page.locator('.mcp-trace-row .mcp-kind-badge').evaluateAll(es=>es.map(e=>e.getBoundingClientRect().width));expect(widths).toHaveLength(3);expect(Math.max(...widths)).toBeLessThanOrEqual(64);
});

test('logging heading filter keyboard does not open an event',async({page})=>{
 await page.setViewportSize({width:1366,height:900});await page.goto('/logging');await expect(page.locator('.mcp-trace-row')).toHaveCount(3);
 const filter=page.locator('.mcp-trace-calm .column-filter').nth(1);const summary=filter.locator('summary');await summary.focus();await summary.press('Enter');
 await expect(filter).toHaveAttribute('open','');await expect(page.locator('.mcp-trace-row.page-item--selected')).toHaveCount(0);
 const input=filter.getByRole('textbox',{name:'Filter Time',exact:true});await input.focus();await input.press('Escape');await expect(filter).not.toHaveAttribute('open','');await expect(summary).toBeFocused();
});

test('logging has one compact page heading and no duplicate summary bars',async({page})=>{
 await page.setViewportSize({width:1366,height:900});await page.goto('/logging');
 await expect(page.getByRole('heading',{name:'Logging',exact:true})).toBeVisible();
 await expect(page.locator('.mcp-trace-project-banner')).toHaveCount(0);
 await expect(page.locator('.mcp-q-strip')).toHaveCount(0);
 await expect(page.getByRole('button',{name:'Open MCP quality',exact:true})).toBeVisible();
 const filter=page.getByRole('search',{name:'Filter MCP log'});
 expect((await filter.boundingBox())!.height).toBeLessThan(150);
});
