import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  diffSelection,
  filterSpecs,
  initialVersion,
  specRouteSearch,
  specRouteTarget,
  type FilterableSpec,
} from './specsPageModel.ts';

describe('initial version', () => {
  const rows = [
    { version: 1, status: 'superseded' },
    { version: 2, status: 'approved' },
    { version: 3, status: 'draft' },
  ];

  it('opens the requested version when it exists', () => {
    assert.equal(initialVersion(rows, 3), 3);
    assert.equal(initialVersion(rows, 1), 1);
  });

  it('falls back to the approved version for a missing or absent request', () => {
    assert.equal(initialVersion(rows, 9), 2);
    assert.equal(initialVersion(rows, null), 2);
  });

  it('falls back to the newest version when none is approved', () => {
    assert.equal(initialVersion([{ version: 1, status: 'draft' }, { version: 4, status: 'draft' }], null), 4);
  });

  it('opens nothing for a document without revisions', () => {
    assert.equal(initialVersion([], null), null);
    assert.equal(initialVersion([], 2), null);
  });
});

describe('route search', () => {
  it('records spec, plan and a non-default tab and keeps other parameters', () => {
    assert.equal(specRouteSearch('?view=documents', 'S1', 'P1', 'plans'), '?view=documents&spec=S1&plan=P1&tab=plans');
  });

  it('leaves the default tab and a missing plan out', () => {
    assert.equal(specRouteSearch('?spec=old&plan=P&tab=plans', 'S2', null, 'agreement'), '?spec=S2');
  });

  it('clears spec, plan and tab when nothing is open', () => {
    assert.equal(specRouteSearch('?spec=S&plan=P&tab=plans&view=documents', null, 'P', 'plans'), '?view=documents');
    assert.equal(specRouteSearch('?spec=S', null, null, 'agreement'), '');
  });

  it('round-trips through the route target', () => {
    const search = specRouteSearch('', 'S/1 x', null, 'authority');
    assert.deepEqual(specRouteTarget(search), { specId: 'S/1 x', planId: null, tab: 'authority' });
  });
});

describe('route target', () => {
  it('is null when the query names no spec', () => {
    assert.equal(specRouteTarget(''), null);
    assert.equal(specRouteTarget('?spec='), null);
    assert.equal(specRouteTarget('?plan=P'), null);
  });

  it('opens the plans tab for a plan whatever tab is named', () => {
    assert.deepEqual(specRouteTarget('?spec=S&plan=P&tab=authority'), { specId: 'S', planId: 'P', tab: 'plans' });
  });

  it('accepts only known tabs and defaults to the agreement', () => {
    assert.equal(specRouteTarget('?spec=S&tab=plans')?.tab, 'plans');
    assert.equal(specRouteTarget('?spec=S&tab=authority')?.tab, 'authority');
    assert.equal(specRouteTarget('?spec=S&tab=bogus')?.tab, 'agreement');
    assert.equal(specRouteTarget('?spec=S')?.tab, 'agreement');
  });
});

describe('filter specs', () => {
  const spec = (id: string, kind: string, latestStatus: string, extra: Partial<FilterableSpec> = {}): FilterableSpec => ({
    id,
    title: `Title ${id}`,
    path: `docs/${id}.md`,
    sessions: '',
    kind,
    latestStatus,
    ...extra,
  });
  const rows = [
    spec('a', 'spec', 'approved'),
    spec('b', 'plan', 'draft', { sessions: 'sess-42' }),
    spec('c', 'spec', 'change_requested', { title: 'Checkout Flow' }),
  ];
  const ids = (out: FilterableSpec[]) => out.map((s) => s.id);

  it('keeps every row with no filters', () => {
    assert.deepEqual(ids(filterSpecs(rows, 'all', 'all', '')), ['a', 'b', 'c']);
  });

  it('filters by kind and status together', () => {
    assert.deepEqual(ids(filterSpecs(rows, 'spec', 'all', '')), ['a', 'c']);
    assert.deepEqual(ids(filterSpecs(rows, 'plan', 'all', '')), ['b']);
    assert.deepEqual(ids(filterSpecs(rows, 'spec', 'approved', '')), ['a']);
    assert.deepEqual(ids(filterSpecs(rows, 'plan', 'approved', '')), []);
  });

  it('searches title, id, path and sessions without case and ignores surrounding spaces', () => {
    assert.deepEqual(ids(filterSpecs(rows, 'all', 'all', '  checkout ')), ['c']);
    assert.deepEqual(ids(filterSpecs(rows, 'all', 'all', 'SESS-42')), ['b']);
    assert.deepEqual(ids(filterSpecs(rows, 'all', 'all', 'docs/a.md')), ['a']);
    assert.deepEqual(ids(filterSpecs(rows, 'all', 'all', 'nothing')), []);
  });
});

describe('diff selection', () => {
  const revs = [{ version: 1 }, { version: 2 }, { version: 3 }];

  it('diffs against the previous revision by default', () => {
    const sel = diffSelection(revs, 3, null);
    assert.equal(sel.current?.version, 3);
    assert.deepEqual(sel.older.map((r) => r.version), [1, 2]);
    assert.equal(sel.base?.version, 2);
  });

  it('uses the chosen base only when it is older than the open revision', () => {
    assert.equal(diffSelection(revs, 3, 1).base?.version, 1);
    assert.equal(diffSelection(revs, 2, 3).base?.version, 1);
  });

  it('has no base for the first revision and no current for an unknown version', () => {
    assert.equal(diffSelection(revs, 1, null).base, null);
    const none = diffSelection(revs, 7, 1);
    assert.equal(none.current, undefined);
    assert.deepEqual(none.older, []);
    assert.equal(none.base, null);
  });
});
