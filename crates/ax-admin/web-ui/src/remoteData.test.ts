import assert from 'node:assert/strict';
import { test } from 'node:test';
import { logPageFrom, toolsFrom } from './remoteData.ts';

test('tools keep their name, text, schema and whether they change data', () => {
  const schema = { type: 'object', properties: {} };
  assert.deepEqual(
    toolsFrom([
      { name: 'ax_status', description: 'Index stats', inputSchema: schema, annotations: { readOnlyHint: true } },
      { name: 'ax_index', annotations: { readOnlyHint: false } },
      { name: 'ax_plain' },
    ]),
    [
      { name: 'ax_status', description: 'Index stats', inputSchema: schema, readOnly: true },
      { name: 'ax_index', description: '', inputSchema: undefined, readOnly: false },
      { name: 'ax_plain', description: '', inputSchema: undefined, readOnly: true },
    ],
  );
});

test('odd tool entries are dropped instead of breaking a page', () => {
  assert.deepEqual(toolsFrom(null), []);
  assert.deepEqual(toolsFrom({ tools: [] }), []);
  const tools = toolsFrom([
    null, 3, 'ax_x', [], {}, { name: 7 }, { name: '' },
    { name: 'ax_ok', description: { html: '<b>x</b>' }, annotations: 'nope' },
    { name: 'ax_ok', description: 'a second entry with the same name' },
  ]);
  assert.deepEqual(tools, [{ name: 'ax_ok', description: '', inputSchema: undefined, readOnly: true }]);
});

test('a log page keeps well-formed rows and the cursor', () => {
  const page = logPageFrom({
    entries: [
      { id: 9, at: 1700000000000, tool: 'ax_sync', transport: 'mcp', ok: true, durationMs: 12, responseTokens: 40 },
      { id: 8, at: 1700000000000, tool: 'ax_index', ok: false, durationMs: null },
      { id: 'x', at: 1, tool: 'ax_bad' },
      { id: 7, at: 1 },
      null,
    ],
    nextBefore: 8,
    limit: 100,
  });
  assert.deepEqual(page, {
    entries: [
      { id: 9, at: 1700000000000, tool: 'ax_sync', transport: 'mcp', ok: true, durationMs: 12, responseTokens: 40 },
      { id: 8, at: 1700000000000, tool: 'ax_index', transport: '', ok: false, durationMs: null, responseTokens: 0 },
    ],
    nextBefore: 8,
  });
  assert.deepEqual(logPageFrom({ entries: [], nextBefore: null }), { entries: [], nextBefore: null });
});

test('anything that is not a log page is null', () => {
  for (const value of [null, undefined, 'text', [], {}, { entries: 'x' }]) assert.equal(logPageFrom(value), null);
});
