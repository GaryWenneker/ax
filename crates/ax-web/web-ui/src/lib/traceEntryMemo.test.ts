import assert from 'node:assert/strict';
import { test } from 'node:test';

import { loggingRowView } from './loggingPresentation.ts';
import {
  entryHasQueryPayload,
  entryMeta,
  entryTextPayloads,
  extractPayloadJson,
  primaryTextPayload,
  type TraceEntry,
} from './mcpTrace.ts';

function entry(message: string, kind: TraceEntry['kind'] = 'inbound'): TraceEntry {
  return {
    id: '1',
    raw: message,
    time: '',
    day: null,
    instantMs: null,
    kind,
    badge: kind,
    message,
    tool: 'ax_explore',
  };
}

function countParses(run: () => void): number {
  const original = JSON.parse;
  let calls = 0;
  JSON.parse = ((text: string, reviver?: (this: unknown, key: string, value: unknown) => unknown) => {
    calls += 1;
    return original(text, reviver);
  }) as typeof JSON.parse;
  try {
    run();
  } finally {
    JSON.parse = original;
  }
  return calls;
}

test('rendering one Logging row again parses its payload no more', () => {
  const e = entry('inbound tool=ax_explore args={"query":"how does sync work","session":"axs_1"}');
  const renderRow = () => {
    loggingRowView(e);
    primaryTextPayload(e);
    entryMeta(e);
    entryHasQueryPayload(e);
  };
  const first = countParses(renderRow);
  assert.ok(first >= 1);
  assert.equal(countParses(renderRow), 0);
});

test('a preview row parses its payload once across all derived views', () => {
  const e = entry('outbound preview tool=ax_explore text={"query":"x","text":"hello"}', 'preview');
  const parses = countParses(() => {
    extractPayloadJson(e);
    entryMeta(e);
    entryTextPayloads(e);
    primaryTextPayload(e);
    entryHasQueryPayload(e);
    loggingRowView(e);
  });
  assert.equal(parses, 1);
});

test('cached views follow the entry when its message changes', () => {
  const e = entry('inbound tool=ax_explore args={"query":"first"}');
  assert.equal(primaryTextPayload(e)?.value, 'first');
  e.message = 'inbound tool=ax_explore args={"query":"second"}';
  assert.equal(primaryTextPayload(e)?.value, 'second');
  assert.deepEqual(extractPayloadJson(e), { query: 'second' });
});

test('callers cannot corrupt the cached text payload list', () => {
  const e = entry('inbound tool=ax_explore args={"query":"keep"}');
  entryTextPayloads(e).length = 0;
  assert.equal(entryTextPayloads(e).length, 1);
});
