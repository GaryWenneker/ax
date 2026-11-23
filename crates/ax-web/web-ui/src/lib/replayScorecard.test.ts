import assert from 'node:assert/strict';
import test from 'node:test';
import { formatReplayNotice, scorecardLabel } from './replayScorecard.ts';

test('scorecard never invents a confident pass for not_tested', () => {
  assert.equal(scorecardLabel('not_tested'), 'Scorecard: not tested (no measured pass)');
  const notice = formatReplayNotice({
    id: 'abcdef0123456789',
    manifest_id: 'm1',
    session_id: 's1',
    turn_id: 't1',
    agent_id: 'a1',
    automated: 'not_implemented',
    score: 'not_tested',
    approval: 'unapproved',
  });
  assert.match(notice, /not tested/);
  assert.match(notice, /not implemented/);
  assert.equal(/95%|passed|confident green/i.test(notice), false);
});
