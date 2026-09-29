import assert from 'node:assert/strict';
import { test } from 'node:test';
import { navigateRoute } from './routes.ts';

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
