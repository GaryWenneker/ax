import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildPath, migrateLegacyHash, navigateRoute, pageFromNavId, parseLocation, parsePathname, type Page, type RouteState } from './routes.ts';

function withWindow(run: (pushed: string[]) => void) {
  const pushed: string[] = [];
  const g = globalThis as unknown as { window?: unknown; PopStateEvent?: unknown };
  const prevWindow = g.window;
  const prevEvent = g.PopStateEvent;
  g.PopStateEvent = class {};
  g.window = {
    location: { pathname: '/', search: '' },
    history: { pushState: (_s: unknown, _t: string, u: string) => pushed.push(u), replaceState: () => {} },
    dispatchEvent: () => true,
  };
  try {
    run(pushed);
  } finally {
    g.window = prevWindow;
    g.PopStateEvent = prevEvent;
  }
}

test('L4 navigateRoute keeps origin=global and projectId in the URL', () => {
  withWindow((pushed) => {
    navigateRoute({ page: 'policy-skills', skillName: 'azdo-pr-review', origin: 'global', projectId: 2 });
    assert.deepEqual(pushed, ['/policy/skills?name=azdo-pr-review&origin=global&projectId=2']);
  });
});

test('navigateRoute leaves a project item URL bare', () => {
  withWindow((pushed) => {
    navigateRoute({ page: 'policy-rules', ruleId: 'r1', origin: null, projectId: null });
    assert.deepEqual(pushed, ['/policy/rules?id=r1']);
  });
});

test('navigateRoute opens a CLI turn memory from Logging', () => {
  withWindow((pushed) => {
    navigateRoute({ page: 'memory', memoryId: 'turn-abc123def456' });
    assert.deepEqual(pushed, ['/memory?memory=turn-abc123def456']);
  });
});

function loc(path: string): Location {
  const url = new URL(path, 'http://localhost');
  return { pathname: url.pathname, search: url.search } as Location;
}

const EMPTY: Omit<RouteState, 'page'> = {
  ruleId: null,
  skillName: null,
  kind: null,
  memoryId: null,
  sonarTab: 'dashboard',
  ruleEditMode: false,
  skillEditMode: false,
  origin: null,
  projectId: null,
  session: null,
  recallQuery: null,
  specId: null,
  section: null,
};

const CASES: Array<Partial<RouteState> & { page: Page }> = [
  { page: 'stats' },
  { page: 'nodes' },
  { page: 'graph' },
  { page: 'files' },
  { page: 'search' },
  { page: 'memory', memoryId: 'turn-1', session: 's 1', recallQuery: 'a&b=c' },
  { page: 'sonar', sonarTab: 'setup' },
  { page: 'settings' },
  { page: 'logging', kind: 'tool', session: 'abc' },
  { page: 'specs', specId: 'full-review-fixes' },
  { page: 'savings' },
  { page: 'prices' },
  { page: 'unresolved', kind: 'calls' },
  { page: 'policy-rules', ruleId: 'r/1' },
  { page: 'policy-rule-edit', ruleId: 'r1', ruleEditMode: true },
  { page: 'policy-skills', skillName: 'old-coder', origin: 'global', projectId: 7 },
  { page: 'policy-skill-edit', skillName: 'old-coder', skillEditMode: true },
  { page: 'policy-match' },
  { page: 'policy-review' },
  { page: 'policy-sync' },
];

for (const state of CASES) {
  test(`parseLocation(buildPath(state)) round-trips ${state.page}`, () => {
    const parsed = parseLocation(loc(buildPath(state)));
    assert.deepEqual(parsed, { ...EMPTY, ...state, valid: true });
  });
}

test('parsePathname accepts / and trailing slashes and flags unknown paths', () => {
  assert.deepEqual(parsePathname('/'), { page: 'stats', valid: true });
  assert.deepEqual(parsePathname('/policy/rules/'), { page: 'policy-rules', valid: true });
  assert.deepEqual(parsePathname('policy/skills'), { page: 'policy-skills', valid: true });
  assert.deepEqual(parsePathname('/nope'), { page: 'stats', valid: false });
});

test('parseLocation ignores edit mode on other pages and a non-numeric projectId', () => {
  const parsed = parseLocation(loc('/policy/rules?id=r1&mode=edit&origin=global&projectId=x'));
  assert.equal(parsed.ruleEditMode, false);
  assert.equal(parsed.projectId, null);
  assert.equal(parsed.origin, 'global');
});

test('buildPath drops parameters that do not belong to the page', () => {
  assert.equal(buildPath({ page: 'stats', ruleId: 'r1', session: 's', specId: 'x', kind: 'k' }), '/stats');
  assert.equal(buildPath({ page: 'policy-rule-edit', ruleEditMode: true }), '/policy/rules/edit');
});

function withHash(hash: string, run: (replaced: string[]) => void) {
  const replaced: string[] = [];
  const g = globalThis as unknown as { window?: unknown };
  const prev = g.window;
  g.window = {
    location: { hash, pathname: '/', search: '' },
    history: { replaceState: (_s: unknown, _t: string, u: string) => replaced.push(u) },
  };
  try {
    run(replaced);
  } finally {
    g.window = prev;
  }
}

test('migrateLegacyHash turns old hash bookmarks into fixed paths', () => {
  withHash('#policy-rule-edit?id=r1&mode=edit', (replaced) => {
    assert.equal(migrateLegacyHash(), '/policy/rules/edit?id=r1&mode=edit');
    assert.deepEqual(replaced, ['/policy/rules/edit?id=r1&mode=edit']);
  });
  withHash('#ship', (replaced) => {
    assert.equal(migrateLegacyHash(), '/stats');
    assert.deepEqual(replaced, ['/stats']);
  });
  withHash('#sonar?tab=setup', () => assert.equal(migrateLegacyHash(), '/sonar?tab=setup'));
});

test('migrateLegacyHash leaves an empty or unknown hash alone', () => {
  withHash('', (replaced) => {
    assert.equal(migrateLegacyHash(), null);
    assert.deepEqual(replaced, []);
  });
  withHash('#unknown?id=1', (replaced) => {
    assert.equal(migrateLegacyHash(), null);
    assert.deepEqual(replaced, []);
  });
});

test('pageFromNavId refuses editor and match pages', () => {
  assert.equal(pageFromNavId('logging'), 'logging');
  assert.equal(pageFromNavId('policy-rule-edit'), null);
  assert.equal(pageFromNavId('policy-match'), null);
  assert.equal(pageFromNavId('ship'), null);
});
