import assert from 'node:assert/strict';
import test from 'node:test';

import {
  applyReport,
  completeActivity,
  contextLabel,
  emptyActivity,
  headline,
  noteTool,
  publicActivityText,
  stripContextReports,
} from './activityModel.ts';

const TAG =
  '<ax_context_report session="a2901fb6-5746-4857-9a52-2e336186d6d4" working="none" stale="false" cache="0"/>';

test('ready context is human text', () => {
  const { reports } = stripContextReports(TAG);
  const label = contextLabel(applyReport(emptyActivity(), reports[0]));
  assert.equal(label, 'Context · Ready');
  assert.equal(label?.includes('stale='), false);
});

test('cached context when cache entries exist', () => {
  const { reports } = stripContextReports(
    '<ax_context_report session="abc" working="none" stale="false" cache="4"/>',
  );
  assert.equal(contextLabel(applyReport(emptyActivity(), reports[0])), 'Context · Cached');
});

test('cached ratio and reused tokens only when those attributes exist', () => {
  const ratio = stripContextReports(
    '<ax_context_report stale="false" cache="1" cacheRatio="0.82"/>',
  ).reports[0];
  assert.equal(contextLabel(applyReport(emptyActivity(), ratio)), 'Context · Cached · 82%');
  const tokens = stripContextReports(
    '<ax_context_report stale="false" cache="1" tokensReused="12400"/>',
  ).reports[0];
  assert.equal(
    contextLabel(applyReport(emptyActivity(), tokens)),
    'Context · Cached · 12.4k tokens reused',
  );
});

test('refreshing context', () => {
  const { reports } = stripContextReports(
    '<ax_context_report session="abc" working="none" stale="true" cache="0"/>',
  );
  const snap = applyReport(emptyActivity(), reports[0]);
  assert.equal(contextLabel(snap), 'Context · Refreshing…');
  assert.equal(snap.phase, 'preparing');
});

test('a fresh report after refresh is rebuilt', () => {
  let snap = applyReport(
    emptyActivity(),
    stripContextReports('<ax_context_report stale="true" cache="0"/>').reports[0],
  );
  snap = applyReport(
    snap,
    stripContextReports('<ax_context_report stale="false" cache="0"/>').reports[0],
  );
  assert.equal(contextLabel(snap), 'Context · Rebuilt');
});

test('tool and search counts are plain numbers', () => {
  let snap = emptyActivity();
  snap = noteTool(snap, 'ax_explore');
  snap = noteTool(snap, 'ax_search');
  snap = noteTool(snap, 'rg');
  for (let i = 0; i < 21; i++) snap = noteTool(snap, 'ax_node');
  assert.equal(snap.tools, 24);
  assert.equal(snap.searches, 3);
  const text = publicActivityText(snap, snap.startedAt + 15000);
  assert.match(text, /\n24\n/);
  assert.match(text, /\n3$/);
  assert.equal(text.includes('{'), false);
});

test('raw context report never stays in assistant text', () => {
  const { text } = stripContextReports(`Hello\n${TAG}\nWorld`);
  assert.equal(text.includes('<ax_context_report'), false);
  assert.match(text, /Hello/);
  assert.match(text, /World/);
});

test('session id stays out of the normal activity text', () => {
  const { reports } = stripContextReports(TAG);
  const text = publicActivityText(applyReport(emptyActivity(0), reports[0]), 8000);
  assert.equal(text.includes('a2901fb6'), false);
  assert.equal(text.includes('<ax_context_report'), false);
  assert.match(text, /Thinking · 8s/);
  assert.match(text, /Context · Ready/);
});

test('completion collapses to one status line', () => {
  let snap = noteTool(noteTool(emptyActivity(0), 'ax_explore'), 'ax_node');
  snap = applyReport(
    snap,
    stripContextReports('<ax_context_report stale="false" cache="2"/>').reports[0],
  );
  snap = completeActivity(snap, 32000);
  const text = publicActivityText(snap, 99000);
  assert.match(headline(snap, 99000), /^Completed · 32s$/);
  assert.match(text, /Context cached · 2 tools · 1 search/);
});

test('repeating a report updates one snapshot', () => {
  const report = stripContextReports(TAG).reports[0];
  const once = applyReport(emptyActivity(), report);
  const twice = applyReport(once, report);
  assert.equal(twice.tools, once.tools);
  assert.equal(contextLabel(twice), contextLabel(once));
});
