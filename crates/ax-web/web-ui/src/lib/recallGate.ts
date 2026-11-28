/** What a memory recall request was sent for, or what the page shows now. */
export interface RecallInputs {
  q: string;
  archive: boolean;
  session: string;
}

/**
 * True when a recall answer sent for `sent` still matches the page's current
 * inputs. A late answer for an older query, the other Archive scope, or a page
 * that has since switched to a session filter must not replace what is shown.
 */
export function recallStillApplies(sent: RecallInputs, now: RecallInputs): boolean {
  return (
    !now.session &&
    sent.session === now.session &&
    sent.archive === now.archive &&
    sent.q.trim() === now.q.trim() &&
    now.q.trim() !== ''
  );
}
