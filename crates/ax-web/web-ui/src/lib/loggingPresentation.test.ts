import assert from 'node:assert/strict';
import {test} from 'node:test';
import {loggingRowView} from './loggingPresentation.ts';
import type {TraceKind} from './mcpTrace';

const domainRows: [TraceKind, string, string][] = [
  ['workspace', 'workspace sync ok files=19 duration_ms=162 tool=workspace', 'Sync · 19 files synced'],
  ['workspace', 'workspace sync ok files=1 tool=workspace', 'Sync · 1 file synced'],
  ['workspace', 'workspace sync start watch=0 tool=workspace', 'Sync started'],
  ['memory', 'memory capture-git ok scanned=20 captured=2 tool=memory', 'Memory · 2 commits captured'],
  ['memory', 'memory capture-git start limit=20 tool=memory', 'Memory · Capture started'],
  ['ship', 'ship ok mode=evaluate passed=1 tool=ship', 'Quality · Check passed'],
  ['ship', 'ship start mode=evaluate tool=ship', 'Quality · Check started'],
  ['ship', 'ship ok mode=ci passed=0 tool=ship', 'Quality · Check failed'],
];
for (const [kind, body, title] of domainRows) test(`readable domain row: ${body}`, () => {
  const entry={kind,message:`[ax] 🪓 ${body}`,tool:kind};
  const original={...entry};
  assert.equal(loggingRowView(entry).title,title);
  assert.deepEqual(entry,original);
});

test('malformed domain facts are not invented', () => {
  const entry={kind:'workspace' as const,message:'[ax] workspace sync ok files=-1 duration_ms=nope tool=workspace',tool:'workspace'};
  assert.equal(loggingRowView(entry).title,'Sync');
});

test('CLI turn.completed activity exposes memory id for Logging → Memory', () => {
  const entry = {
    kind: 'cli' as const,
    message:
      '[ax] 🪓 cli activity op=turn.completed status=success source=cli id=e1 key=turn.finish:t1 session=axs_1 turn=t1 summary=Root cause found memory=turn-abcdef0123456789',
    tool: 'cli',
  };
  assert.equal(loggingRowView(entry).memoryId, 'turn-abcdef0123456789');
});
