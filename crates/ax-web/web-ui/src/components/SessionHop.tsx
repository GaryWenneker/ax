import { useEffect, useState } from 'react';
import { navigateRoute } from '../lib/routes';
import { createSpecCatalog, type SpecLink } from '../lib/specCatalog';
import { isChatSession, sessionHopTargets, specForSession } from '../lib/specSession';
import { WORKSPACE_SWITCHED } from '../workspaceEvents';

const specCatalog = createSpecCatalog();

/** Logging, Memory, and the plan for one session. The current page's own button is omitted. */
export function SessionHop({ session, here }: { session: string; here: 'memory' | 'logging' }) {
  const [plan, setPlan] = useState<SpecLink | null>(null);
  const [workspace, setWorkspace] = useState(0);
  useEffect(() => {
    // Another project has other specs: drop the cached list and look again.
    const switched = () => {
      specCatalog.invalidate();
      setPlan(null);
      setWorkspace((n) => n + 1);
    };
    window.addEventListener(WORKSPACE_SWITCHED, switched);
    return () => window.removeEventListener(WORKSPACE_SWITCHED, switched);
  }, []);
  useEffect(() => {
    if (!isChatSession(session)) {
      setPlan(null);
      return undefined;
    }
    let cancel = false;
    void specCatalog.load().then((specs) => {
      if (!cancel) setPlan(specForSession(specs, session) ?? null);
    });
    return () => {
      cancel = true;
    };
  }, [session, workspace]);
  const targets = sessionHopTargets(session, here, plan);
  if (targets.length === 0) return null;
  const title = plan?.title || 'Plan';
  return (
    <span className="session-hop">
      {targets.map((target) =>
        target.label === 'Plan' ? (
          <button
            key={target.label}
            type="button"
            className="specs-jump"
            title={title}
            aria-label={`Open plan ${title}`}
            onClick={() => navigateRoute(target.route)}
          >
            Plan
          </button>
        ) : (
          <button key={target.label} type="button" className="specs-jump" onClick={() => navigateRoute(target.route)}>
            {target.label}
          </button>
        ),
      )}
    </span>
  );
}
