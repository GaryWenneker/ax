import { test } from 'node:test';
import assert from 'node:assert/strict';
import { AUTO_GROUP_THRESHOLD, proposeGroups, type AutoGroupDef, type AutoGroupItem } from './autoGroup.ts';

const groups: AutoGroupDef[] = [
  { id: 'git', label: 'Git & pull requests', aliases: ['pr'] },
  { id: 'testing', label: 'Testing & evidence' },
];

const items: AutoGroupItem[] = [
  { key: 'pr', text: 'Open a draft pull request with a short description', group: 'git' },
  { key: 'commit', text: 'Write conventional commit messages for every pull request', group: 'git' },
  { key: 'tdd', text: 'Write the failing test first, then the implementation', group: 'testing' },
  { key: 'new-review', text: 'Review a pull request and its commit messages', group: 'ungrouped' },
  { key: 'new-mutant', text: 'Mutation testing proves every test can fail', group: 'ungrouped' },
  { key: 'new-weather', text: 'Sunny weather forecast banana', group: 'ungrouped' },
];

test('only ungrouped items get proposals', () => {
  const keys = proposeGroups(items, groups).map((p) => p.key);
  assert.deepEqual(keys, ['new-review', 'new-mutant', 'new-weather']);
});

test('the most similar existing group wins', () => {
  const byKey = new Map(proposeGroups(items, groups).map((p) => [p.key, p]));
  assert.equal(byKey.get('new-review')!.group, 'git');
  assert.equal(byKey.get('new-mutant')!.group, 'testing');
  assert.ok(byKey.get('new-review')!.score >= AUTO_GROUP_THRESHOLD);
});

test('an item below the threshold stays ungrouped', () => {
  const weather = proposeGroups(items, groups).find((p) => p.key === 'new-weather')!;
  assert.equal(weather.group, null);
  assert.ok(weather.score < AUTO_GROUP_THRESHOLD);
});

test('no groups means no group proposals', () => {
  for (const p of proposeGroups(items, [])) assert.equal(p.group, null);
});

test('the ungrouped bucket is never a target', () => {
  const withUngrouped = [...groups, { id: 'ungrouped', label: 'Ungrouped' }];
  for (const p of proposeGroups(items, withUngrouped)) assert.notEqual(p.group, 'ungrouped');
});

test('group label and aliases count, so an empty group can still match', () => {
  const only = [{ key: 'x', text: 'Deploy the site to production hosting', group: 'ungrouped' }];
  const [p] = proposeGroups(only, [{ id: 'deploy', label: 'Deploy & hosting' }, { id: 'git', label: 'Git' }]);
  assert.equal(p.group, 'deploy');
});

test('the same input gives the same result', () => {
  assert.deepEqual(proposeGroups(items, groups), proposeGroups(items, groups));
});
