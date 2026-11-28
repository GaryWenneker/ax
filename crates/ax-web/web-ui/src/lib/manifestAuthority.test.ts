import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { canStartReplay, uncertaintyLabel } from './manifestAuthority.ts';

const panel = readFileSync(new URL('../components/ManifestAuthorityPanel.tsx', import.meta.url), 'utf8');

test('uncertainty labels stay readable without inventing confidence', () => {
  assert.equal(uncertaintyLabel('unapproved'), 'Unapproved');
  assert.equal(uncertaintyLabel('integrity_failed'), 'Integrity failed');
  assert.equal(uncertaintyLabel('not_run'), 'Replay not run');
  assert.equal(uncertaintyLabel('not_tested'), 'Replay not tested');
  assert.equal(uncertaintyLabel('replica_stale'), 'replica stale');
});

test('a replay starts only from an intact seal while no other replay is starting', () => {
  assert.equal(canStartReplay({ integrity_ok: true }, null), true);
  assert.equal(canStartReplay({ integrity_ok: false }, null), false);
  assert.equal(canStartReplay({ integrity_ok: true }, 'manifest-1'), false);
  assert.equal(canStartReplay({ integrity_ok: false }, 'manifest-1'), false);
});

/** The panel's Replay button and its start handler both call canStartReplay with the same arguments. */
function gatesShareCheck(source: string): boolean {
  const args = (match: RegExpMatchArray | null) => match?.[1].replace(/\s+/g, '') ?? null;
  const button = args(source.match(/disabled=\{\s*!canStartReplay\(([^)]*)\)\s*\}/));
  const start = args(source.match(/if\s*\(\s*!canStartReplay\(([^)]*)\)\s*\)\s*return\b/));
  return button !== null && button === start;
}

test('the panel gates the Replay button and the start on the same check and shows the receipt once', () => {
  assert.equal(gatesShareCheck(panel), true);
  assert.equal(panel.match(/formatReplayNotice\(/g)?.length, 1);
});

test('the gate check follows the call, not the local variable names', () => {
  const renamed = panel.replace(/\bbusyId\b/g, 'startingId').replace(/\brow\b/g, 'item');
  assert.notEqual(renamed, panel);
  assert.equal(gatesShareCheck(renamed), true);
  const ungated = panel.replace(/if \(!canStartReplay\([^)]*\)\) return;/, '');
  assert.equal(gatesShareCheck(ungated), false);
  const mismatched = panel.replace(/disabled=\{!canStartReplay\([^)]*\)\}/, 'disabled={!canStartReplay(row, null)}');
  assert.equal(gatesShareCheck(mismatched), false);
});
