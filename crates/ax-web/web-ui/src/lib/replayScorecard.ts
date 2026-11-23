export type ReplayRunView = {
  id: string;
  manifest_id: string;
  session_id: string;
  turn_id: string;
  agent_id: string;
  automated: string;
  score: string;
  approval: string;
};

/** Human label for a stored replay score. Never invents a pass. */
export function scorecardLabel(score: string): string {
  switch (score) {
    case 'not_tested':
      return 'Scorecard: not tested (no measured pass)';
    case 'not_run':
      return 'Scorecard: not run';
    default:
      return `Scorecard: ${score.replace(/_/g, ' ')}`;
  }
}

export function automatedLabel(automated: string): string {
  switch (automated) {
    case 'not_implemented':
      return 'Automated replay: not implemented';
    default:
      return `Automated replay: ${automated.replace(/_/g, ' ')}`;
  }
}

export function formatReplayNotice(run: ReplayRunView): string {
  return [
    `Replay ${run.id.slice(0, 12)}…`,
    automatedLabel(run.automated),
    scorecardLabel(run.score),
    `Approval: ${run.approval}`,
  ].join(' · ');
}
