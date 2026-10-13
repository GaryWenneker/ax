import { test, expect } from '@playwright/test';
import { chooseSelect } from './chooseSelect';
import { SETTINGS_GROUPS } from '../src/lib/settingsNavigation';

function localExport(client: string) {
  const vscode=client==='vscode';
  const server={command:'ax',args:['serve','--mcp','--path','/demo'],...(vscode?{type:'stdio'}:{})};
  return {workspaceId:'fixture',client,target:'thisComputer',content:JSON.stringify({[vscode?'servers':'mcpServers']:{ax:server}},null,2),fileName:'mcp.json',destination:vscode?'.vscode/mcp.json':'.cursor/mcp.json',instructions:['Paste the ax entry into your app configuration.'],containsSecrets:false,containsLocalPaths:true,portable:false};
}

test('shared dropdowns and paragraph action gaps work in both themes', async ({ page }) => {
  test.setTimeout(120_000);
  await page.route('**/api/**', async route => {
    const path = new URL(route.request().url()).pathname;
    const data = path.endsWith('/local/export') ? localExport(route.request().postDataJSON().client)
      : path.replace(/\/$/,'') === '/api/remote-mcp/publications' ? {workspaceId:'fixture',projectLabel:'Demo',csrf:'csrf',publications:[]}
      : path.endsWith('/publications/host') ? {workspaceId:'fixture',host:null}
      : path.endsWith('/host/authorizations') ? {workspaceId:'fixture',requests:[]}
      : path === '/api/access/session' ? {csrf:'csrf'}
      : path === '/api/access/nodes' ? {nodes:[],local:null}
      : path === '/api/access/nodes/local' ? {settings:{config:{node_id:'primary',role:'primary',cluster_id:'office',projects:[{id:'project'}]},port:9070,advertise:'http://192.168.1.20:9070'},running:true,active_project:true,active_replicas:0,sync:null}
      : path === '/api/stats' ? {readonly:false,project_name:'Demo',node_count:0,edge_count:0,file_count:0,languages:[]}
      : path === '/api/share/status' ? {readonly:false}
      : path === '/api/ship/config' ? {config:{ship:{target_branch:'main',web_port:7070},quality_gate:{steps:[],tests:{runner:'cargo test'}},remote:{provider:'github'},ui:{}},sonar:{}}
      : null;
    await route.fulfill({status:data?200:404,json:data??{error:'Fixture endpoint not provided'}});
  });
  for (const theme of ['azure-light','azure-dark']) {
    await page.goto('/settings');
    await page.evaluate(id=>localStorage.setItem('ax-theme',id),theme);
    await page.reload();
    await expect(page.locator('html')).toHaveAttribute('data-ax-theme',theme);
    await page.locator('[aria-controls="settings-panel-remote"]').click();
    const app=page.getByLabel('Local MCP app');
    await expect(app).toBeEnabled();
    await chooseSelect(app, 'vscode');
    await expect(app).toHaveAttribute('data-value', 'vscode');
    await expect(page.locator('.local-mcp-config')).toContainText('.vscode/mcp.json');
    for (const {id:section} of SETTINGS_GROUPS.flatMap(group=>group.items)) {
      await page.locator(`[aria-controls="settings-panel-${section}"]`).click();
      const invalid=await page.locator('select:visible:not([multiple]):not([size])').evaluateAll(elements=>elements.filter(element=>{
        const style=getComputedStyle(element);
        return style.appearance!=='none'||parseFloat(style.paddingInlineEnd)<36||!style.backgroundImage.includes('data:image/svg+xml');
      }).map(element=>element.outerHTML.slice(0,180)));
      expect(invalid,`${theme}/${section}`).toEqual([]);
      expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
    }
    await page.locator('[aria-controls="settings-panel-nodes"]').click();
    const saved=page.getByLabel('Saved replication configuration');
    const note=await saved.locator('.access-note').boundingBox();
    const button=await saved.getByRole('button',{name:'Remove local configuration'}).boundingBox();
    expect(note).not.toBeNull();expect(button).not.toBeNull();
    expect(button!.y-(note!.y+note!.height)).toBeGreaterThanOrEqual(15);
    if(process.env.AX_ACCESS_SCREENSHOTS&&test.info().project.name==='desktop-chrome') {
      await saved.screenshot({path:`../../../site/public/screenshots/cc-replication-controls-${theme}.png`});
      await page.locator('[aria-controls="settings-panel-remote"]').click();
      await page.locator('.local-mcp-config').screenshot({path:`../../../site/public/screenshots/cc-local-controls-${theme}.png`});
    }
  }
  await page.locator('[aria-controls="settings-panel-remote"]').click();
  let failedExports=0;
  await page.route('**/publications/local/export', async route=>{
    failedExports++;
    await route.fulfill({status:404,json:{error:'Local configuration unavailable'}});
  });
  await page.getByRole('button',{name:'Refresh project publications'}).click();
  await expect(page.getByText('Local configuration unavailable',{exact:true})).toBeVisible();
  await expect(page.getByRole('heading',{name:'Use on this computer',exact:true})).toHaveCount(1);
  expect(failedExports).toBeLessThanOrEqual(2);
});

test('revoked OAuth and replica records can be removed after confirmation', async ({ page }) => {
  let oauthRemoved=false,nodeRemoved=false;
  await page.route('**/api/**', async route=>{
    const path=new URL(route.request().url()).pathname;
    if(path.endsWith('/host/authorizations/revoked')&&route.request().method()==='POST') {
      expect(route.request().headers()['x-ax-csrf']).toBe('csrf');
      expect(route.request().postDataJSON()).toEqual({workspaceId:'fixture',decision:'remove'});
      oauthRemoved=true;await route.fulfill({json:{workspaceId:'fixture',updated:true}});return;
    }
    if(path.endsWith('/nodes/revoked/remove')) {
      expect(route.request().headers()['x-ax-access-csrf']).toBe('csrf');
      nodeRemoved=true;await route.fulfill({json:{removed:true}});return;
    }
    const data=path.endsWith('/local/export')?localExport(route.request().postDataJSON().client)
      :path.replace(/\/$/,'')==='/api/remote-mcp/publications'?{workspaceId:'fixture',projectLabel:'Demo',csrf:'csrf',publications:[]}
      :path.endsWith('/publications/host')?{workspaceId:'fixture',host:{status:'running',message:null,publicUrl:'https://ax.example.com',localUrl:'http://127.0.0.1:9000',builtInOAuth:true,autoStart:true}}
      :path.endsWith('/host/authorizations')?{workspaceId:'fixture',requests:oauthRemoved?[]:[{id:'revoked',clientName:'Old IDE',scope:'ax:context',status:'delivered',expires:9999999999,revoked:true,redirectUri:'http://127.0.0.1:8000/callback'}]}
      :path==='/api/access/session'?{csrf:'csrf'}
      :path==='/api/access/nodes'?{nodes:nodeRemoved?[]:[{id:'revoked',display_name:'Old replica',role:'replica',trust_state:'revoked',endpoint:null,last_seen_at:null,last_sync_at:null}],local:null}
      :path==='/api/access/nodes/local'?{settings:null,running:false,active_project:false}
      :path==='/api/share/status'?{readonly:false}
      :path==='/api/stats'?{readonly:false,project_name:'Demo',node_count:0,edge_count:0,file_count:0,languages:[]}:null;
    await route.fulfill({status:data?200:404,json:data??{error:'Fixture endpoint not provided'}});
  });
  await page.goto('/settings');
  await page.locator('[aria-controls="settings-panel-remote"]').click();
  const host=page.getByRole('region',{name:'Remote MCP host'});
  await host.getByRole('button',{name:'Remove connection'}).click();
  await page.getByRole('dialog').getByRole('button',{name:'Cancel',exact:true}).click();
  expect(oauthRemoved).toBe(false);
  await host.getByRole('button',{name:'Remove connection'}).click();
  await page.getByRole('dialog').getByRole('button',{name:'Remove connection',exact:true}).click();
  await expect(host).not.toContainText('Old IDE');
  await page.locator('[aria-controls="settings-panel-nodes"]').click();
  await page.getByRole('button',{name:'Remove revoked node',exact:true}).click();
  await page.getByRole('dialog').getByRole('button',{name:'Remove revoked node',exact:true}).click();
  await expect(page.getByText('Old replica',{exact:true})).toHaveCount(0);
  expect(nodeRemoved).toBe(true);
});
