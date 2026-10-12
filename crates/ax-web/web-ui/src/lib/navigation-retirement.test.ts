import assert from 'node:assert/strict';
import test from 'node:test';
import { visibleNav } from './nav.ts';
import { parsePathname, pageFromNavId } from './routes.ts';
test('Activity retains useful pages and excludes retired Agent', () => {
  for (const savings of [true, false]) {
    const activity = visibleNav(savings).find(section => section.id === 'activity');
    assert.ok(activity);
    assert.equal(activity.items.some(item => String(item.id) === 'agent'), false);
    assert.ok(activity.items.some(item => item.id === 'logging'));
  }
  assert.deepEqual(parsePathname('/agent'), { page: 'stats', valid: false });
  assert.equal(pageFromNavId('agent'), null);
});
test('Quality Gates page and navigation are retired', () => {
  assert.deepEqual(parsePathname('/ship'), { page: 'stats', valid: false });
  assert.equal(pageFromNavId('ship'), null);
  for (const savings of [true, false]) assert.equal(visibleNav(savings).flatMap(s => s.items).some(i => String(i.id) === 'ship'), false);
});
