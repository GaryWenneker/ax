import assert from 'node:assert/strict';
import {test} from 'node:test';
import {loggingRowPresentation} from './loggingPresentation.ts';
import type {TraceKind} from './mcpTrace';

const cases: [TraceKind, string, string, string][] = [
  ['workspace', 'workspace sync ok files=19 duration_ms=162 tool=workspace', 'Workspace sync completed', '19 files · 162 ms'],
  ['workspace', 'workspace sync start watch=0 tool=workspace', 'Workspace sync started', 'Watch off'],
  ['memory', 'memory capture-git ok scanned=20 captured=2 tool=memory', 'Git history captured', '20 scanned · 2 captured'],
  ['memory', 'memory capture-git start limit=20 tool=memory', 'Git history capture started', 'Limit 20'],
  ['ship', 'ship ok mode=evaluate passed=1 tool=ship', 'Quality check completed', 'Evaluate · Passed'],
  ['ship', 'ship start mode=evaluate tool=ship', 'Quality check started', 'Evaluate'],
  ['ship', 'ship ok mode=ci passed=0 tool=ship', 'Quality check completed', 'CI · Failed'],
  ['workspace', 'workspace sync start watch=1 files=0 duration_ms=0 tool=workspace', 'Workspace sync started', '0 files · 0 ms · Watch on'],
  ['memory', 'memory capture-git ok scanned=0 captured=0 tool=memory', 'Git history captured', '0 scanned · 0 captured'],
  ['ship', 'ship-ci ok mode=preview passed=unknown watch=unknown tool=ship-ci', 'Quality check completed', 'preview'],
];
for (const [kind, body, title, subtitle] of cases) test(`readable domain row: ${body}`, () => {
  const entry={kind,message:`[ax] 🪓 ${body}`,tool:kind};
  const original={...entry};
  assert.deepEqual(loggingRowPresentation(entry,{title:entry.message,subtitle:kind}),{title,subtitle});
  assert.deepEqual(entry,original);
});

test('unknown and failed domain messages retain their complete content', () => {
  for (const message of ['[ax] 🪓 workspace sync failed reason=disk-full tool=workspace', '[ax] 🪓 workspace custom detail=some_value']) {
    const entry={kind:'workspace' as const,message,tool:'workspace'};
    assert.deepEqual(loggingRowPresentation(entry,{title:message,subtitle:'Workspace'}),{title:message.replace('[ax] 🪓 ',''),subtitle:'Workspace'});
  }
});

test('human text is not rewritten as a log line', () => {
  const entry={kind:'inbound' as const,message:'inbound tool=ax_explore args={}',tool:'ax_explore'};
  const text='[ax] workspace sync ok files=19 is the text I want to search';
  assert.deepEqual(loggingRowPresentation(entry,{title:'fallback',subtitle:'Prompt in · JSON',text}),{title:text,subtitle:'Prompt in · JSON'});
});

test('absent, negative and malformed facts are not invented', () => {
  const entry={kind:'workspace' as const,message:'[ax] workspace sync ok files=-1 duration_ms=nope tool=workspace',tool:'workspace'};
  assert.deepEqual(loggingRowPresentation(entry,{title:entry.message,subtitle:'Workspace'}),{title:'Workspace sync completed',subtitle:''});
});

test('other kinds and empty text keep the fallback without logger decoration', () => {
  const entry={kind:'other' as const,message:'[ax-mcp] 🪓 something unexpected detail=retain',tool:null};
  assert.deepEqual(loggingRowPresentation(entry,{title:entry.message,subtitle:'Other',text:''}),{title:'something unexpected detail=retain',subtitle:'Other'});
});
