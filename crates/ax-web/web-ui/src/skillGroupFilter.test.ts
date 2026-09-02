import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  allListedCollapsed,
  allListedExpanded,
  collapseAllGroupIds,
  expandAllGroupIds,
  matchesGroupFilter,
  toggleGroupFilterId,
} from './skillGroupFilter.ts';

describe('matchesGroupFilter', () => {
  it('F1 empty selection matches any group', () => {
    assert.equal(matchesGroupFilter('testing', []), true);
    assert.equal(matchesGroupFilter('git', []), true);
  });

  it('F2 selected groups hide others', () => {
    assert.equal(matchesGroupFilter('testing', ['testing']), true);
    assert.equal(matchesGroupFilter('git', ['testing']), false);
  });

  it('F3 multiselect is OR', () => {
    const sel = ['testing', 'git'];
    assert.equal(matchesGroupFilter('testing', sel), true);
    assert.equal(matchesGroupFilter('git', sel), true);
    assert.equal(matchesGroupFilter('ungrouped', sel), false);
  });
});

describe('toggleGroupFilterId', () => {
  it('adds then removes', () => {
    const one = toggleGroupFilterId([], 'testing');
    assert.deepEqual(one, ['testing']);
    assert.deepEqual(toggleGroupFilterId(one, 'testing'), []);
  });
});

describe('collapse/expand all', () => {
  it('F4 collapse all stores every listed id', () => {
    const ids = ['testing', 'git'];
    const set = collapseAllGroupIds(ids);
    assert.equal(set.size, 2);
    assert.equal(set.has('testing'), true);
    assert.equal(allListedCollapsed([], new Set()), true);
  });

  it('F5 expand all is empty', () => {
    const ids = ['testing', 'git'];
    const set = expandAllGroupIds();
    assert.equal(set.size, 0);
    assert.equal(allListedExpanded(ids, set), true);
    assert.equal(allListedCollapsed(ids, set), false);
  });
});
