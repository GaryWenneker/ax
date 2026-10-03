import { test } from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import {
  NO_WORKSPACE_FOLDER,
  PanelHost,
  STATUS_BAR_TEXT,
  STATUS_BAR_TOOLTIP,
  commandCenterUrl,
  launchArgs,
  popoutRows,
  preparePopout,
  rowAction,
  sameProjectPath,
  switchProject,
  webviewHtml,
  workspaceProjectPath,
  type RecentProject,
} from '../src/core.ts';

test('webview html has csp frame-src for port only', () => {
  const html = webviewHtml(7071, 'n0nce');
  assert.equal(commandCenterUrl(7071), 'http://127.0.0.1:7071/?embed=1');
  assert.match(html, /<iframe src="http:\/\/127\.0\.0\.1:7071\/\?embed=1"/);
  assert.match(html, /iframe\{position:absolute;inset:0/);
  const csp = /http-equiv="Content-Security-Policy" content="([^"]+)"/.exec(html)?.[1] ?? '';
  assert.match(csp, /default-src 'none'/);
  assert.match(csp, /frame-src http:\/\/127\.0\.0\.1:7071;/);
  assert.match(csp, /style-src 'nonce-n0nce'/);
  assert.doesNotMatch(csp, /\*|https?:\/\/(?!127\.0\.0\.1:7071)/);
});

test('webview html rejects a port that is not a number in range', () => {
  assert.throws(() => webviewHtml(0, 'n'), /port/);
  assert.throws(() => webviewHtml(70700, 'n'), /port/);
  assert.throws(() => webviewHtml(7070.5, 'n'), /port/);
});

test('panel is reused on second open', () => {
  let created = 0;
  let revealed = 0;
  let reloaded = 0;
  let dispose: () => void = () => {};
  const host = new PanelHost(() => {
    created++;
    return {
      reveal: () => revealed++,
      reload: () => reloaded++,
      onDispose: (fn) => (dispose = fn),
    };
  });
  host.open();
  host.open();
  assert.equal(created, 1);
  assert.equal(revealed, 1);
  assert.equal(reloaded, 1);
  dispose();
  host.open();
  assert.equal(created, 2);
});

const axRoot = path.resolve('/projects/ax');
const ioRoot = path.resolve('/projects/io');
const nested = path.resolve('/projects/ax/ide/vscode');

function hasDb(roots: readonly string[]): (root: string) => boolean {
  const set = new Set(roots.map((root) => path.resolve(root)));
  return (root) => set.has(path.resolve(root));
}

test('status bar label stays ax and the tooltip names this workspace', () => {
  assert.equal(STATUS_BAR_TEXT, '$(graph) ax');
  assert.equal(STATUS_BAR_TOOLTIP, 'Open ax Command Center for this workspace');
});

test('workspace path is the open folder when it has an ax database', () => {
  assert.equal(workspaceProjectPath([axRoot], undefined, hasDb([axRoot])), axRoot);
});

test('active file selects its workspace folder, not the first folder', () => {
  const active = path.join(axRoot, 'ide', 'vscode', 'src', 'core.ts');
  assert.equal(workspaceProjectPath([ioRoot, axRoot], active, hasDb([ioRoot, axRoot])), axRoot);
});

test('a nested workspace folder resolves to the parent ax project', () => {
  assert.equal(workspaceProjectPath([nested], undefined, hasDb([axRoot])), axRoot);
});

test('no workspace folder does not invent a project path', () => {
  assert.equal(workspaceProjectPath([], undefined, hasDb([axRoot])), undefined);
});

test('a folder with no ax database stays that folder so the hub can reject it', () => {
  assert.equal(workspaceProjectPath([nested], undefined, hasDb([])), nested);
});

test('same project path ignores a trailing separator', () => {
  assert.equal(sameProjectPath(axRoot, axRoot + path.sep), true);
  assert.equal(sameProjectPath(axRoot, ioRoot), false);
});

test('pop-out lists Open Command Center first and omits the workspace and uninitialized projects', () => {
  const recent: RecentProject[] = [
    { path: ioRoot, label: 'io', initialized: true },
    { path: axRoot + path.sep, label: 'ax', initialized: true },
    { path: path.resolve('/projects/fresh'), label: 'fresh', initialized: false },
  ];
  assert.deepEqual(popoutRows(axRoot, recent), [
    { kind: 'open', label: 'Open Command Center', detail: axRoot, path: axRoot },
    { kind: 'switch', label: 'io', detail: ioRoot, path: ioRoot },
  ]);
});

test('accepting Open Command Center does not switch again', () => {
  assert.deepEqual(rowAction({ kind: 'open', label: 'Open Command Center', detail: axRoot, path: axRoot }), {
    openPanel: true,
  });
});

test('accepting another project switches to that path and opens the panel', () => {
  assert.deepEqual(rowAction({ kind: 'switch', label: 'io', detail: ioRoot, path: ioRoot }), {
    openPanel: true,
    switchPath: ioRoot,
  });
});

test('cold start launches ax web with the workspace project', () => {
  assert.deepEqual(launchArgs(7070, axRoot), ['web', '--port', '7070', axRoot]);
});

test('click with no folder does not start the server or call the hub', async () => {
  let called = false;
  const result = await preparePopout({
    port: 7070,
    folders: [],
    hasAxDb: hasDb([axRoot]),
    ensureServer: async () => {
      called = true;
    },
    fetchImpl: async () => {
      called = true;
      throw new Error('fetch');
    },
  });
  assert.equal(called, false);
  assert.deepEqual(result, { ok: false, kind: 'folder', message: NO_WORKSPACE_FOLDER });
});

test('click switches to the workspace project before the pop-out is ready', async () => {
  const calls: string[] = [];
  const result = await preparePopout({
    port: 7070,
    folders: [axRoot],
    hasAxDb: hasDb([axRoot]),
    ensureServer: async (_port, project) => {
      calls.push(`start ${project}`);
    },
    fetchImpl: async (url, init) => {
      const method = init?.method ?? 'GET';
      calls.push(`${method} ${String(url)}`);
      if (method === 'GET') {
        return jsonResponse({
          ok: true,
          workspace: { path: ioRoot, label: 'io', initialized: true },
          recent: [
            { path: ioRoot, label: 'io', initialized: true },
            { path: axRoot, label: 'ax', initialized: true },
          ],
        });
      }
      assert.equal(init?.body, JSON.stringify({ path: axRoot }));
      return jsonResponse({ ok: true, path: axRoot, label: 'ax', switched: true });
    },
  });
  assert.deepEqual(calls, [
    `start ${axRoot}`,
    'GET http://127.0.0.1:7070/api/workspace/current',
    'POST http://127.0.0.1:7070/api/workspace/switch',
  ]);
  assert.equal(result.ok, true);
  if (result.ok) {
    assert.equal(result.switched, true);
    assert.equal(result.rows[0]?.label, 'Open Command Center');
    assert.equal(result.rows[0]?.path, axRoot);
  }
});

test('click skips switch when the hub is already on this workspace', async () => {
  const methods: string[] = [];
  const result = await preparePopout({
    port: 7070,
    folders: [axRoot],
    hasAxDb: hasDb([axRoot]),
    ensureServer: async () => {},
    fetchImpl: async (_url, init) => {
      methods.push(init?.method ?? 'GET');
      return jsonResponse({
        ok: true,
        workspace: { path: axRoot, label: 'ax', initialized: true },
        recent: [{ path: ioRoot, label: 'io', initialized: true }],
      });
    },
  });
  assert.deepEqual(methods, ['GET']);
  assert.deepEqual(result, {
    ok: true,
    switched: false,
    rows: [
      { kind: 'open', label: 'Open Command Center', detail: axRoot, path: axRoot },
      { kind: 'switch', label: 'io', detail: ioRoot, path: ioRoot },
    ],
  });
});

test('a failed switch does not return pop-out rows', async () => {
  const result = await preparePopout({
    port: 7070,
    folders: [axRoot],
    hasAxDb: hasDb([axRoot]),
    ensureServer: async () => {},
    fetchImpl: async (_url, init) => {
      if ((init?.method ?? 'GET') === 'GET') {
        return jsonResponse({
          ok: true,
          workspace: { path: ioRoot, label: 'io', initialized: true },
          recent: [],
        });
      }
      return jsonResponse({ ok: false, error: 'Project not initialized — run init first' });
    },
  });
  assert.deepEqual(result, {
    ok: false,
    kind: 'hub',
    message: 'Project not initialized — run init first',
  });
});

test('a server that does not start does not return pop-out rows', async () => {
  const result = await preparePopout({
    port: 7070,
    folders: [axRoot],
    hasAxDb: hasDb([axRoot]),
    ensureServer: async () => {
      throw new Error('no answer on port 7070 after 15 s');
    },
    fetchImpl: async () => {
      throw new Error('should not fetch');
    },
  });
  assert.deepEqual(result, {
    ok: false,
    kind: 'start',
    message: 'no answer on port 7070 after 15 s',
  });
});

test('switch project reports a dead hub and an unreadable reply', async () => {
  const dead = await switchProject(7070, ioRoot, async () => {
    throw new Error('connection refused');
  });
  assert.deepEqual(dead, { ok: false, message: 'connection refused' });

  const unreadable = await switchProject(7070, ioRoot, async () => new Response('nope', { status: 200 }));
  assert.deepEqual(unreadable, { ok: false, message: 'Command Center did not switch project.' });
});

test('click reports a hub that is down or that refuses the switch', async () => {
  const base = {
    port: 7070,
    folders: [axRoot],
    hasAxDb: hasDb([axRoot]),
    ensureServer: async () => {},
  };
  const down = await preparePopout({
    ...base,
    fetchImpl: async () => {
      throw new Error('connection refused');
    },
  });
  assert.deepEqual(down, { ok: false, kind: 'hub', message: 'connection refused' });

  const refused = await preparePopout({
    ...base,
    fetchImpl: async () => jsonResponse({ ok: false, error: 'Read-only mode' }),
  });
  assert.deepEqual(refused, { ok: false, kind: 'hub', message: 'Read-only mode' });
});

test('a recent row without a path is left out of the pop-out', async () => {
  const result = await preparePopout({
    port: 7070,
    folders: [axRoot],
    hasAxDb: hasDb([axRoot]),
    ensureServer: async () => {},
    fetchImpl: async () =>
      jsonResponse({
        ok: true,
        workspace: { path: axRoot, label: 'ax', initialized: true },
        recent: [null, { label: 'nameless' }, { path: ioRoot, label: 'io', initialized: true }],
      }),
  });
  assert.deepEqual(result, {
    ok: true,
    switched: false,
    rows: [
      { kind: 'open', label: 'Open Command Center', detail: axRoot, path: axRoot },
      { kind: 'switch', label: 'io', detail: ioRoot, path: ioRoot },
    ],
  });
});

test('switch project posts the path to the hub', async () => {
  let body = '';
  const result = await switchProject(7070, ioRoot, async (url, init) => {
    assert.equal(String(url), 'http://127.0.0.1:7070/api/workspace/switch');
    assert.equal(init?.method, 'POST');
    body = String(init?.body);
    return jsonResponse({ ok: true });
  });
  assert.equal(body, JSON.stringify({ path: ioRoot }));
  assert.deepEqual(result, { ok: true });
});

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });
}
