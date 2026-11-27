import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { readGlobalStyles } from '../../test-support/globalStyles.ts';

// Behaviour of the Specs page is tested in ../lib/specsPageModel.test.ts,
// ../lib/specDocLoad.test.ts, ../lib/specSort.test.ts and the Playwright
// specs e2e/specs-page.spec.ts and e2e/specs-unsaved.spec.ts. These few checks
// only pin wiring that has no runtime test in this suite.

const routes = readFileSync(new URL('../lib/routes.ts', import.meta.url), 'utf8');
const nav = readFileSync(new URL('../lib/nav.ts', import.meta.url), 'utf8');
const css = readGlobalStyles();

test('the specs page has a route and a navigation entry', () => {
  assert.match(routes, /specs: '\/specs'/);
  assert.match(nav, /id: 'specs', label: 'Specs & Plans'/);
});

test('the specs list pane scrolls on its own', () => {
  assert.match(css, /\.specs-page \.specs-list-pane \{[^}]*overflow:\s*auto/);
});
