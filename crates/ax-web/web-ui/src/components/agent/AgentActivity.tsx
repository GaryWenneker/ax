import { useEffect, useState } from 'react';

import Codicon from '../Codicon';
import {
  contextFace,
  contextLabel,
  headline,
  summaryLine,
  type ActivitySnapshot,
} from './activityModel';

export default function AgentActivity({ snapshot }: { snapshot: ActivitySnapshot }) {
  const [now, setNow] = useState(() => Date.now());
  const live = snapshot.phase !== 'completed';

  useEffect(() => {
    if (!live) return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [live]);

  const face = contextFace(snapshot);
  const context = contextLabel(snapshot);
  const summary = summaryLine(snapshot);
  const showStats = snapshot.phase !== 'thinking' || snapshot.tools > 0 || context != null;
  const titleIcon = snapshot.phase === 'completed' ? 'check' : snapshot.phase === 'preparing' ? 'sync' : 'sparkle';

  return (
    <article
      className={`agent-activity agent-activity--${snapshot.phase}${face === 'refreshing' ? ' agent-activity--refreshing' : ''}${live ? ' agent-activity-live' : ''}`}
      aria-live="polite"
    >
      <header className="agent-activity-head">
        <Codicon name={titleIcon} className={snapshot.phase === 'preparing' ? 'codicon-modifier-spin' : undefined} />
        <span className="agent-activity-title">{headline(snapshot, now)}</span>
      </header>

      {showStats && (
        <div className="agent-activity-stats">
          {context && (
            <div className={`agent-activity-stat agent-activity-stat--context${face === 'refreshing' ? ' is-live' : ''}`}>
              <Codicon name={face === 'refreshing' ? 'sync' : face === 'cached' ? 'database' : 'circle-filled'} className={face === 'refreshing' ? 'codicon-modifier-spin' : undefined} />
              <span>
                <span className="agent-activity-kicker">Context</span>
                <span className="agent-activity-value">{context.replace(/^Context · /, '')}</span>
              </span>
            </div>
          )}
          {snapshot.tools > 0 && (
            <div className="agent-activity-stat">
              <Codicon name="zap" />
              <span>
                <span className="agent-activity-kicker">Tools</span>
                <span className="agent-activity-value">{snapshot.tools}</span>
              </span>
            </div>
          )}
          {snapshot.searches > 0 && (
            <div className="agent-activity-stat">
              <Codicon name="search" />
              <span>
                <span className="agent-activity-kicker">Searches</span>
                <span className="agent-activity-value">{snapshot.searches}</span>
              </span>
            </div>
          )}
        </div>
      )}

      {summary && <p className="agent-activity-summary">{summary}</p>}

      {snapshot.context && (
        <details className="agent-activity-details">
          <summary>Context details</summary>
          <dl>
            <div><dt>Status</dt><dd>{context?.replace(/^Context · /, '')}</dd></div>
            <div><dt>Cache</dt><dd>{snapshot.context.cache > 0 ? 'Available' : 'None yet'}</dd></div>
            {snapshot.context.session && (
              <div><dt>Session</dt><dd>{snapshot.context.session}</dd></div>
            )}
            <div><dt>Working</dt><dd>{snapshot.context.working && snapshot.context.working !== 'none' ? snapshot.context.working : 'None'}</dd></div>
            <div><dt>Stale</dt><dd>{snapshot.context.stale ? 'Yes' : 'No'}</dd></div>
          </dl>
        </details>
      )}
    </article>
  );
}
