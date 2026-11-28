import assert from 'node:assert/strict';
import { test } from 'node:test';
import { PAGES, hasAdminAccess, pageFromHash, visiblePages } from './pages.ts';

/** The tools the Management pages are for. */
const MANAGEMENT_TOOLS = PAGES.filter((p) => p.section === 'Management').flatMap((p) => p.tools);
const tool = (name: string, admin = false) => ({ name, admin });

const CONTEXT = ['ax_preflight', 'ax_status', 'ax_search', 'ax_node', 'ax_explore', 'ax_rules', 'ax_recall', 'ax_session'];

test('a context-only server shows no management pages', () => {
  const ids = visiblePages(CONTEXT).map((p) => p.id);
  assert.deepEqual(ids, ['status', 'search', 'nodes', 'explore', 'rules', 'memory', 'sessions', 'tools']);
  assert.equal(hasAdminAccess(CONTEXT.map((name) => tool(name))), false);
});

test('an admin grant adds the management pages', () => {
  const ids = visiblePages([...CONTEXT, ...MANAGEMENT_TOOLS]).map((p) => p.id);
  for (const id of ['operations', 'files', 'usage', 'logging'] as const) assert.ok(ids.includes(id), id);
});

test('admin access follows what the server says about its tools, not their names', () => {
  // A tool this version has never heard of, marked as admin by the server.
  assert.equal(hasAdminAccess([tool('ax_status'), tool('ax_compact', true)]), true);
  // A management tool name the server does not mark as admin.
  assert.equal(hasAdminAccess([tool('ax_status'), tool('ax_logs')]), false);
  assert.equal(hasAdminAccess([]), false);
});

test('one listed tool is enough for its page, and Tools is always there', () => {
  assert.deepEqual(visiblePages(['ax_callers']).map((p) => p.id), ['nodes', 'tools']);
  assert.deepEqual(visiblePages(['ax_remember']).map((p) => p.id), ['memory', 'tools']);
  assert.deepEqual(visiblePages([]).map((p) => p.id), ['tools']);
  assert.deepEqual(visiblePages(['ax_future_tool']).map((p) => p.id), ['tools']);
});

test('every management tool belongs to one page only', () => {
  for (const name of MANAGEMENT_TOOLS) {
    const owners = PAGES.filter((p) => p.tools.includes(name));
    assert.equal(owners.length, 1, name);
  }
});

test('the hash selects a visible page and falls back to the first one', () => {
  const pages = visiblePages(CONTEXT);
  assert.equal(pageFromHash('#/memory', pages), 'memory');
  assert.equal(pageFromHash('#memory', pages), 'memory');
  assert.equal(pageFromHash('', pages), 'status');
  assert.equal(pageFromHash('#/operations', pages), 'status');
  assert.equal(pageFromHash('#/nope', visiblePages([])), 'tools');
});
