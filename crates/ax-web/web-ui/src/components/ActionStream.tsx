import { useEffect, useState } from 'react';
import { subscribeSharedEventSource } from '../lib/sharedEventSource';
import { liveClass, useNewKeys } from '../lib/useLive';

type ActionEvent = {
  ts: number;
  kind: string;
  message: string;
};

const MAX = 8;

/** Live agent/MCP/graph action strip (SSE `/api/actions/events`). */
export default function ActionStream() {
  const [events, setEvents] = useState<ActionEvent[]>([]);
  const shown = events.slice(0, 3);
  const fresh = useNewKeys(shown.map(actionKey));

  useEffect(() => {
    return subscribeSharedEventSource('/api/actions/events', {
      events: {
        action: (ev) => {
          try {
            const data = JSON.parse((ev as MessageEvent).data) as ActionEvent;
            if (data.kind === 'stream') return;
            setEvents((prev) => [data, ...prev].slice(0, MAX));
          } catch {
            /* ignore malformed */
          }
        },
      },
    });
  }, []);

  if (events.length === 0) return null;

  return (
    <div className="action-stream" aria-live="polite">
      {shown.map((e) => (
        <div key={actionKey(e)} className={`action-stream__item${liveClass(fresh, actionKey(e))}`}>
          <span className="action-stream__kind">{e.kind}</span>
          <span className="action-stream__msg">{e.message}</span>
        </div>
      ))}
    </div>
  );
}

function actionKey(e: ActionEvent): string {
  return `${e.ts}-${e.kind}-${e.message}`;
}
