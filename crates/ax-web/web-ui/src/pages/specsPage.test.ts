import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { readGlobalStyles } from '../../test-support/globalStyles.ts';

// Behaviour of the Specs page is tested in ../lib/specsPageModel.test.ts,
// ../lib/specDocLoad.test.ts, ../lib/specSort.test.ts and the Playwright
// specs e2e/specs-page.spec.ts and e2e/specs-unsaved.spec.ts. These few checks
// only pin wiring that has no runtime test in this suite.

const page = readFileSync(new URL('./Specs.tsx', import.meta.url), 'utf8');
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

test('the specs page rescans the plan folders on every load', () => {
  // fetchSpecList({ refresh: true }) requests /api/specs?refresh=1 (see pageApis.test.ts).
  assert.match(page, /await fetchSpecList\(\{ refresh: true \}\)/);
  assert.doesNotMatch(page, /await fetchSpecList\(\)/);
});

test('the specs blade slides in like the policy blade', () => {
  assert.match(css, /\.memory-blade\.specs-blade \{[^}]*policy-blade-slide-in/);
});
