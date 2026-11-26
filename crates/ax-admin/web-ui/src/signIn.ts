// Waiting for a browser sign-in. One request at a time: the next question is
// asked only after the previous answer arrived, so a slow answer can never be
// overtaken by a later one.

export type SignInStatus =
  | { state: 'pending' }
  | { state: 'ready' }
  | { state: 'failed'; message: string };

export type SignInOutcome<Connection> =
  | { state: 'connected'; connection: Connection }
  | { state: 'failed'; message: string }
  /** Nobody waits any more. `saved` is a connection that was saved just before. */
  | { state: 'stopped'; saved: Connection | null };

export interface SignInSteps<Connection> {
  /** Where the sign-in stands. Reads only. */
  status: () => Promise<SignInStatus>;
  /** Saves the connection once the sign-in is ready. */
  complete: () => Promise<{ state: 'connected'; connection: Connection } | { state: 'failed'; message: string }>;
  /** Resolves after the pause between two questions. */
  pause: () => Promise<void>;
  /** True once nobody waits for the outcome any more. */
  stopped: () => boolean;
}

export async function waitForSignIn<Connection>(steps: SignInSteps<Connection>): Promise<SignInOutcome<Connection>> {
  for (;;) {
    if (steps.stopped()) return { state: 'stopped', saved: null };
    let status: SignInStatus;
    try {
      status = await steps.status();
    } catch (e) {
      return { state: 'failed', message: e instanceof Error ? e.message : 'Sign-in failed.' };
    }
    if (steps.stopped()) return { state: 'stopped', saved: null };
    if (status.state === 'failed') return status;
    if (status.state === 'ready') {
      let done: Awaited<ReturnType<SignInSteps<Connection>['complete']>>;
      try {
        done = await steps.complete();
      } catch (e) {
        if (steps.stopped()) return { state: 'stopped', saved: null };
        return { state: 'failed', message: e instanceof Error ? e.message : 'Sign-in failed.' };
      }
      // Stopped while the connection was being saved: say so, with what was
      // saved, so the caller can take it back. No "connected" for a view that is gone.
      if (steps.stopped()) return { state: 'stopped', saved: done.state === 'connected' ? done.connection : null };
      return done;
    }
    await steps.pause();
  }
}
