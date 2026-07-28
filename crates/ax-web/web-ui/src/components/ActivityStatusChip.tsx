import { useEffect, useRef, useState } from 'react';
import { subscribeSharedEventSource } from '../lib/sharedEventSource';
import Codicon from './Codicon';
import { navigateRoute } from '../lib/routes';

type ActionEvent = {
  ts: number;
  kind: string;
  message: string;
  meta?: unknown;
};

const MAX = 12;

type Props = {
  openPanel: string | null;
  onTogglePanel: (id: string | null) => void;
};

function relativeTime(ts: number, now: number): string {
  const sec = Math.max(0, Math.round((now - ts) / 1000));
  if (sec < 5) return 'just now';
  if (sec < 60) return `${sec}s ago`;
  const min = Math.round(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.round(min / 60);
  if (hr < 48) return `${hr}h ago`;
  return new Date(ts).toLocaleString();
}

function kindClass(kind: string): string {
  const k = kind.toLowerCase();
  if (k === 'lsp' || k === 'enrich') return 'lsp';
  if (k === 'ship' || k === 'ship-ci' || k === 'gate') return 'ship';
  if (k === 'plugin') return 'plugin';
  if (k === 'error' || k === 'fail') return 'danger';
  if (k === 'workspace' || k === 'share') return 'share';
  return 'default';
}

/** Status-bar activity chip + popover (replaces floating ActionStream). */
export default function ActivityStatusChip({ openPanel, onTogglePanel }: Props) {
  const [events, setEvents] = useState<ActionEvent[]>([]);
  const [unread, setUnread] = useState(0);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const isOpen = openPanel === 'activity';
  const openRef = useRef(isOpen);
  openRef.current = isOpen;

  useEffect(() => {
    return subscribeSharedEventSource('/api/actions/events', {
      events: {
        action: (ev) => {
          try {
            const data = JSON.parse((ev as MessageEvent).data) as ActionEvent;
            if (data.kind === 'stream') return;
            setEvents((prev) => [data, ...prev].slice(0, MAX));
            if (!openRef.current) setUnread((n) => n + 1);
          } catch {
            /* ignore */
          }
        },
      },
    });
  }, []);

  useEffect(() => {
    if (isOpen) setUnread(0);
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;
    const id = window.setInterval(() => setNow(Date.now()), 15000);
    return () => clearInterval(id);
  }, [isOpen]);

  const latest = events[0];

  return (
    <span className={`status-chip-wrap${isOpen ? ' status-chip-wrap--open' : ''}`}>
      <button
        type="button"
        className={`status-item status-item--clickable status-activity${isOpen ? ' status-item--active' : ''}${unread > 0 ? ' status-activity--unread' : ''}`}
        title="Live activity"
        aria-expanded={isOpen}
        aria-label={unread > 0 ? `Live activity, ${unread} unread` : 'Live activity'}
        onClick={() => onTogglePanel(isOpen ? null : 'activity')}
      >
        <Codicon name="bell" />
        <span className="status-lbl">Activity</span>
        <span className="status-val">{unread > 0 ? unread : events.length || '—'}</span>
      </button>
      {isOpen && (
        <div
          className="status-panel status-panel--activity"
          role="region"
          aria-label="Live activity"
          aria-live="polite"
        >
          <div className="status-panel-head">
            <strong>Live activity</strong>
            <button
              type="button"
              className="status-panel-link"
              onClick={() => {
                setEvents([]);
                setUnread(0);
                setExpanded(null);
              }}
            >
              Clear
            </button>
          </div>
          {events.length === 0 ? (
            <p className="status-panel-muted">
              No recent events. Ship, LSP enrich, and workspace switches appear here.
            </p>
          ) : (
            <ul className="status-panel-list">
              {events.map((e) => {
                const key = `${e.ts}-${e.kind}-${e.message}`;
                const hasMeta = e.meta != null && e.meta !== '';
                const open = expanded === key;
                return (
                  <li key={key}>
                    <button
                      type="button"
                      className="status-activity-row"
                      onClick={() => {
                        onTogglePanel(null);
                        navigateRoute({ page: 'logging', kind: e.kind || null });
                      }}
                    >
                      <span className="status-activity-row__top">
                        <span
                          className={`status-activity-kind status-activity-kind--${kindClass(e.kind)}`}
                        >
                          {e.kind}
                        </span>
                        <span className="status-activity-time">{relativeTime(e.ts, now)}</span>
                      </span>
                      <span className="status-activity-msg">{e.message}</span>
                    </button>
                    {hasMeta && (
                      <button
                        type="button"
                        className="status-activity-meta-toggle"
                        onClick={(ev) => {
                          ev.stopPropagation();
                          setExpanded(open ? null : key);
                        }}
                      >
                        {open ? 'Hide details' : 'Show details'}
                      </button>
                    )}
                    {open && hasMeta && (
                      <pre className="status-activity-meta mono">
                        {typeof e.meta === 'string'
                          ? e.meta
                          : JSON.stringify(e.meta, null, 2)}
                      </pre>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
          {latest && (
            <p className="status-panel-muted">
              Latest: {latest.kind} — {latest.message}
            </p>
          )}
        </div>
      )}
    </span>
  );
}
