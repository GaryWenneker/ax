import assert from 'node:assert/strict';
import { test } from 'node:test';
import { PAGES, buildPath, migrateLegacyHash, navigateRoute, pageFromNavId, parseLocation, parsePathname } from './routes.ts';

function withWindow(hash: string, run: (urls: string[]) => void) {
  const urls: string[] = [];
  const g = globalThis as unknown as { window?: unknown; PopStateEvent?: unknown };
  const prevWindow = g.window;
  const prevEvent = g.PopStateEvent;
  g.PopStateEvent = class {};
  g.window = {
    location: { pathname: '/', search: '', hash },
    history: {
      pushState: (_s: unknown, _t: string, u: string) => urls.push(u),
      replaceState: (_s: unknown, _t: string, u: string) => urls.push(u),
    },
    dispatchEvent: () => true,
  };
  try {
    run(urls);
  } finally {
    g.window = prevWindow;
    g.PopStateEvent = prevEvent;
  }
}

test('every page has a path that parses back to the same page', () => {
  assert.ok(PAGES.length >= 20);
  for (const page of PAGES) {
    assert.deepEqual(parsePathname(buildPath({ page })), { page, valid: true }, page);
  }
});

test('every page is reachable from its legacy hash and every sidebar page from its nav id', () => {
  const editorOnly = new Set(['policy-rule-edit', 'policy-skill-edit', 'policy-match']);
  for (const page of PAGES) {
    withWindow(`#${page}`, (urls) => {
      assert.equal(migrateLegacyHash(), buildPath({ page }), page);
      assert.deepEqual(urls, [buildPath({ page })]);
    });
    assert.equal(pageFromNavId(page), editorOnly.has(page) ? null : page, page);
  }
  withWindow('#ship', () => assert.equal(migrateLegacyHash(), '/stats'));
  withWindow('#agent', () => assert.equal(migrateLegacyHash(), '/stats'));
});

test('the settings section travels in the route', () => {
  assert.equal(buildPath({ page: 'settings', section: 'agents' }), '/settings?section=agents');
  assert.equal(buildPath({ page: 'stats', section: 'agents' }), '/stats');
  const parsed = parseLocation({ pathname: '/settings', search: '?section=remote' } as Location);
  assert.equal(parsed.section, 'remote');
  assert.equal(buildPath(parsed), '/settings?section=remote');
  withWindow('', (urls) => {
    navigateRoute({ page: 'settings', section: 'plugins' });
    assert.deepEqual(urls, ['/settings?section=plugins']);
  });
  withWindow('#settings?section=vault', () => {
    assert.equal(migrateLegacyHash(), '/settings?section=vault');
  });
});
