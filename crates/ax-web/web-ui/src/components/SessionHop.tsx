import { useEffect, useState } from 'react';
import { navigateRoute } from '../lib/routes';
import { isChatSession, specForSession } from '../lib/specSession';

type SpecLink = { id: string; title: string; sessions: string };

let specCatalog: Promise<SpecLink[]> | null = null;

function loadSpecCatalog(): Promise<SpecLink[]> {
  if (!specCatalog) {
    specCatalog = fetch('/api/specs')
      .then((response) => response.json())
      .then((data: { specs?: SpecLink[] }) => (Array.isArray(data.specs) ? data.specs : []))
      .catch(() => []);
  }
  return specCatalog;
}

/** Logging, Memory, and the plan for one session. The current page's own button is omitted. */
export function SessionHop({ session, here }: { session: string; here: 'memory' | 'logging' }) {
  const [plan, setPlan] = useState<SpecLink | null>(null);
  useEffect(() => {
    if (!isChatSession(session)) {
      setPlan(null);
      return undefined;
    }
    let cancel = false;
    void loadSpecCatalog().then((specs) => {
      if (!cancel) setPlan(specForSession(specs, session) ?? null);
    });
    return () => {
      cancel = true;
    };
  }, [session]);
  if (!isChatSession(session)) return null;
  const title = plan?.title || 'Plan';
  return (
    <span className="session-hop">
      {here !== 'logging' && (
        <button type="button" className="specs-jump" onClick={() => navigateRoute({ page: 'logging', session })}>
          Logging
        </button>
      )}
      {here !== 'memory' && (
        <button type="button" className="specs-jump" onClick={() => navigateRoute({ page: 'memory', session })}>
          Memory
        </button>
      )}
      {plan && (
        <button
          type="button"
          className="specs-jump"
          title={title}
          aria-label={`Open plan ${title}`}
          onClick={() => navigateRoute({ page: 'specs', specId: plan.id })}
        >
          Plan
        </button>
      )}
    </span>
  );
}
