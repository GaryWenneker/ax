export type ManifestSummary = {
  id: string;
  session_id: string;
  turn_id: string;
  agent_id: string;
  approval: string;
  integrity_ok: boolean;
  replay_status: string;
  uncertainties: string[];
};

export function uncertaintyLabel(code: string): string {
  switch (code) {
    case 'unapproved':
      return 'Unapproved';
    case 'integrity_failed':
      return 'Integrity failed';
    case 'not_run':
      return 'Replay not run';
    case 'not_tested':
      return 'Replay not tested';
    default:
      return code.replace(/_/g, ' ');
  }
}

export function canStartReplay(row: Pick<ManifestSummary, 'integrity_ok'>, busyId: string | null): boolean {
  return row.integrity_ok && busyId === null;
}
