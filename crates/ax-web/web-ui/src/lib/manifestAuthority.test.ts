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

test('the panel gates the Replay button and the start on the same check and shows the receipt once', () => {
  assert.match(panel, /disabled=\{!canStartReplay\(row, busyId\)\}/);
  assert.match(panel, /if \(!canStartReplay\(row, busyId\)\) return;/);
  assert.equal(panel.match(/formatReplayNotice\(/g)?.length, 1);
});
