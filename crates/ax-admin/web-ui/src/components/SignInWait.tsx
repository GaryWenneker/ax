import { useEffect, useRef } from 'react';
import { cancelLogin, completeLogin, loginStatus, type Connection, type StartedSignIn } from '../api';
import { waitForSignIn } from '../signIn';

interface Props {
  signIn: StartedSignIn;
  /** The sign-in page did not open by itself (the browser blocked the new tab). */
  blocked: boolean;
  onConnected: (connection: Connection) => void;
  onFailed: (message: string) => void;
  /** The user gave up; the form comes back. */
  onCancelled: () => void;
}

/** Cancels that are scheduled but not yet sent, by sign-in id. */
const pendingCancel = new Map<string, number>();

/**
 * Shown while the user signs in in another tab. It asks ax-admin where the
 * sign-in stands, one question at a time. Leaving this view in any way stops
 * asking and cancels the sign-in, so no tokens are stored for it afterwards.
 */
export default function SignInWait({ signIn, blocked, onConnected, onFailed, onCancelled }: Props) {
  const handlers = useRef({ onConnected, onFailed });
  useEffect(() => {
    handlers.current = { onConnected, onFailed };
  });

  useEffect(() => {
    // React's development mode mounts, unmounts and mounts again: a cancel
    // that was only just scheduled for this sign-in is withdrawn.
    window.clearTimeout(pendingCancel.get(signIn.id));
    pendingCancel.delete(signIn.id);
    let stopped = false;
    let settled = false;
    void waitForSignIn<Connection>({
      status: () => loginStatus(signIn.id),
      complete: () => completeLogin(signIn.id),
      pause: () => new Promise((resolve) => window.setTimeout(resolve, 1000)),
      stopped: () => stopped,
    }).then((outcome) => {
      settled = outcome.state !== 'stopped';
      if (outcome.state === 'connected') handlers.current.onConnected(outcome.connection);
      else if (outcome.state === 'failed') handlers.current.onFailed(outcome.message);
    });
    return () => {
      stopped = true;
      // Closed or cancelled while waiting: tell ax-admin, which also removes
      // tokens if the browser finishes later. Nothing to do once it settled.
      if (settled) return;
      const id = signIn.id;
      pendingCancel.set(id, window.setTimeout(() => {
        pendingCancel.delete(id);
        // Already finished or expired: ax-admin answers "not found", which is fine.
        cancelLogin(id).catch(() => undefined);
      }, 0));
    };
  }, [signIn.id]);

  return (
    <div className="admin-form" role="status" aria-live="polite">
      <p>
        {blocked
          ? 'Your browser did not open the sign-in page. Open it with the link below; this page continues when the environment has granted access.'
          : 'Finish signing in in the browser tab that just opened. This page continues when the environment has granted access.'}
      </p>
      <p className="admin-note">
        <a href={signIn.authorizationUrl} target="_blank" rel="noopener noreferrer">Open the sign-in page</a>
      </p>
      <div className="admin-actions">
        <button type="button" className="btn btn-secondary" autoFocus onClick={onCancelled}>Cancel sign-in</button>
      </div>
    </div>
  );
}
