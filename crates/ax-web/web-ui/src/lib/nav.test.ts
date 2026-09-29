import assert from 'node:assert/strict';
import { test } from 'node:test';
import { navItemActive, visibleNav } from './nav.ts';

function labels(showSavings = true): string[] {
  return visibleNav(showSavings).flatMap((section) => section.items.map((item) => item.label));
}

function sectionLabels(): string[] {
  return visibleNav(true).map((section) => section.label);
}

test('sidebar groups most-used policy and code above activity and system', () => {
  assert.deepEqual(sectionLabels(), ['Policy', 'Code', 'Activity', 'System']);
  const items = labels();
  assert.deepEqual(items.slice(0, 3), ['Rules', 'Skills', 'Memory']);
  assert.ok(items.indexOf('Graph') < items.indexOf('Logging'));
  assert.ok(items.indexOf('Logging') < items.indexOf('Settings'));
  assert.ok(items.indexOf('Settings') < items.indexOf('Prices'));
  assert.equal(items.at(-1), 'Prices');
});

test('Command Center is not a navigation item', () => {
  const items = visibleNav(true).flatMap((section) => section.items);
  assert.equal(items.some((item) => item.id === 'ship'), false);
  assert.equal(items.some((item) => item.label === 'Command Center'), false);
});

test('savings stays in Activity and drops out when hidden', () => {
  const activity = visibleNav(true).find((section) => section.id === 'activity');
  assert.ok(activity?.items.some((item) => item.id === 'savings'));
  const hidden = visibleNav(false).flatMap((section) => section.items.map((item) => item.id));
  assert.equal(hidden.includes('savings'), false);
  assert.equal(hidden.includes('logging'), true);
});

test('rule and skill editors keep their nav item active', () => {
  assert.equal(navItemActive('policy-rule-edit', 'policy-rules'), true);
  assert.equal(navItemActive('policy-skill-edit', 'policy-skills'), true);
  assert.equal(navItemActive('policy-rules', 'policy-skills'), false);
  assert.equal(navItemActive('ship', 'settings'), false);
});
