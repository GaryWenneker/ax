import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { isGitShared, originQs, policyDbAccent, policyDbRowStyle, policyOverviewMenuItems, filterRules, hydratePolicyListItem, sortSkills } from './components/ui/policyListUtils.ts';
import { menuTargets, nextRowSelection, toggleVisibleSelection } from './lib/policySelection.ts';

describe('originQs', () => {
  it('Q1 project origin has no query', () => {
    assert.equal(originQs(undefined), '');
    assert.equal(originQs({ origin: 'project' }), '');
  });

  it('Q2 global origin includes projectId', () => {
    assert.equal(originQs({ origin: 'global', projectId: 7 }), '?origin=global&projectId=7');
  });
});

describe('isGitShared', () => {
  it('G1 project enabled is git-shared', () => {
    assert.equal(isGitShared('project', true), true);
    assert.equal(isGitShared(undefined, true), true);
  });

  it('G2 workspace enabled is git-shared', () => {
    assert.equal(isGitShared('Workspace', true), true);
  });

  it('G3 private, company, and user are not git-shared', () => {
    assert.equal(isGitShared('private', true), false);
    assert.equal(isGitShared('company', true), false);
    assert.equal(isGitShared('user', true), false);
  });

  it('G4 disabled is never git-shared even for project', () => {
    assert.equal(isGitShared('project', false), false);
    assert.equal(isGitShared('workspace', false), false);
  });
});

describe('policyDbAccent', () => {
  it('D1 project ax.db is teal', () => {
    assert.equal(policyDbAccent(undefined), '#3ee4b2');
    assert.equal(policyDbAccent('project'), '#3ee4b2');
  });

  it('D2 global.db is gold', () => {
    assert.equal(policyDbAccent('global'), '#e0b341');
  });

  it('D3 row inset uses the same accent', () => {
    assert.equal(policyDbRowStyle('global').boxShadow, 'inset 5px 0 0 #e0b341');
    assert.equal(policyDbRowStyle(undefined).boxShadow, 'inset 5px 0 0 #3ee4b2');
  });
});

describe('policyOverviewMenuItems', () => {
  it('M1 project menu includes move to global', () => {
    const ids = policyOverviewMenuItems('project', true).map((i) => i.id);
    assert.deepEqual(ids, ['open', 'edit', 'disable', 'move-global', 'delete']);
  });

  it('M2 disabled project offers enable', () => {
    assert.equal(policyOverviewMenuItems(undefined, false).find((i) => i.id === 'enable')?.label, 'Enable');
  });

  it('M3 global menu includes open, edit, move back, delete', () => {
    const ids = policyOverviewMenuItems('global', true).map((i) => i.id);
    assert.deepEqual(ids, ['open', 'edit', 'move-project', 'delete']);
  });
});

describe('filterRules', () => {
  it('F1 nested/missing arrays do not crash', () => {
    const row = {
      id: 'parked',
      level: 'INFO',
      alwaysApply: false,
      priority: 50,
      body: '',
      sourcePath: '',
      origin: 'global' as const,
    };
    const out = filterRules([row as never], { q: '', level: '', always: '' });
    assert.equal(out.length, 1);
  });
});

describe('hydratePolicyListItem', () => {
  it('H1 nested skill frontmatter yields name and tags', () => {
    const row = hydratePolicyListItem({
      origin: 'global',
      body: 'x',
      frontmatter: { name: 'systematic-debugging', tags: ['debugging'], triggers: ['bug'] },
    });
    assert.equal(row.name, 'systematic-debugging');
    assert.deepEqual(row.tags, ['debugging']);
    assert.deepEqual(row.triggers, ['bug']);
  });
});

describe('list after relocate payload', () => {
  const nested = {
    origin: 'global',
    body: 'x',
    frontmatter: { name: 'systematic-debugging', tags: ['debugging'] },
  };

  it('P5 sortSkills does not throw when name is only in frontmatter', () => {
    const other = { name: 'zebra', origin: 'project', tags: [] };
    const sorted = sortSkills([nested as never, other as never], 'name', 'asc');
    assert.equal(sorted.length, 2);
  });

  it('P5 hydrate then sort has a comparable name', () => {
    const row = hydratePolicyListItem(nested);
    const sorted = sortSkills([row as never], 'name', 'asc');
    assert.equal(sorted[0]?.name, 'systematic-debugging');
  });
});

describe('policy row selection', () => {
  const ids = ['a', 'b', 'c'];

  it('S1 plain click selects one and opens it', () => {
    const r = nextRowSelection({
      id: 'b',
      visibleIds: ids,
      selected: new Set(['a']),
      anchor: 'a',
      metaKey: false,
      shiftKey: false,
    });
    assert.deepEqual([...r.selected], ['b']);
    assert.equal(r.openId, 'b');
    assert.equal(r.anchor, 'b');
  });

  it('S1 cmd click toggles into a multi-selection and closes the editor', () => {
    const r = nextRowSelection({
      id: 'c',
      visibleIds: ids,
      selected: new Set(['a']),
      anchor: 'a',
      metaKey: true,
      shiftKey: false,
    });
    assert.equal(r.selected.has('a'), true);
    assert.equal(r.selected.has('c'), true);
    assert.equal(r.openId, null);
  });

  it('S1 shift click selects an inclusive visible range', () => {
    const r = nextRowSelection({
      id: 'c',
      visibleIds: ids,
      selected: new Set(['a']),
      anchor: 'a',
      metaKey: false,
      shiftKey: true,
    });
    assert.deepEqual([...r.selected].sort(), ['a', 'b', 'c']);
    assert.equal(r.openId, null);
  });

  it('S1 header toggle selects all then none', () => {
    const all = toggleVisibleSelection(ids, new Set());
    assert.equal(all.size, 3);
    assert.equal(toggleVisibleSelection(ids, all).size, 0);
  });

  it('S1 context menu uses the multi-selection when the row is already selected', () => {
    const rows = [{ name: 'a' }, { name: 'b' }, { name: 'c' }];
    const hits = menuTargets(rows[1], new Set(['a', 'b']), rows, (r) => r.name);
    assert.deepEqual(hits.map((h) => h.name), ['a', 'b']);
  });
});
