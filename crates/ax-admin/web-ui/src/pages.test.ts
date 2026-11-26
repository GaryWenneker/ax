import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ADMIN_TOOLS, PAGES, hasAdminAccess, pageFromHash, visiblePages } from './pages.ts';

const CONTEXT = ['ax_preflight', 'ax_status', 'ax_search', 'ax_node', 'ax_explore', 'ax_rules', 'ax_recall', 'ax_session'];

test('a context-only server shows no management pages', () => {
  const ids = visiblePages(CONTEXT).map((p) => p.id);
  assert.deepEqual(ids, ['status', 'search', 'nodes', 'explore', 'rules', 'memory', 'sessions', 'tools']);
  assert.equal(hasAdminAccess(CONTEXT), false);
});

test('an admin grant adds the management pages', () => {
  const ids = visiblePages([...CONTEXT, ...ADMIN_TOOLS]).map((p) => p.id);
  for (const id of ['operations', 'files', 'usage', 'logging']) assert.ok(ids.includes(id as never), id);
  assert.equal(hasAdminAccess([...CONTEXT, 'ax_logs']), true);
});

test('one listed tool is enough for its page, and Tools is always there', () => {
  assert.deepEqual(visiblePages(['ax_callers']).map((p) => p.id), ['nodes', 'tools']);
  assert.deepEqual(visiblePages(['ax_remember']).map((p) => p.id), ['memory', 'tools']);
  assert.deepEqual(visiblePages([]).map((p) => p.id), ['tools']);
  assert.deepEqual(visiblePages(['ax_future_tool']).map((p) => p.id), ['tools']);
});

test('every admin tool belongs to a management page and to no other', () => {
  for (const tool of ADMIN_TOOLS) {
    const owners = PAGES.filter((p) => p.tools.includes(tool));
    assert.equal(owners.length, 1, tool);
    assert.equal(owners[0].section, 'Management', tool);
  }
  const managed = PAGES.filter((p) => p.section === 'Management').flatMap((p) => p.tools);
  assert.deepEqual([...managed].sort(), [...ADMIN_TOOLS].sort());
});

test('the hash selects a visible page and falls back to the first one', () => {
  const pages = visiblePages(CONTEXT);
  assert.equal(pageFromHash('#/memory', pages), 'memory');
  assert.equal(pageFromHash('#memory', pages), 'memory');
  assert.equal(pageFromHash('', pages), 'status');
  assert.equal(pageFromHash('#/operations', pages), 'status');
  assert.equal(pageFromHash('#/nope', visiblePages([])), 'tools');
});
