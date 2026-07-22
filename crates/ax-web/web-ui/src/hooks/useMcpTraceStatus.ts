import { useEffect, useRef, useState } from 'react';

import { MCP_TRACE_EVENTS_URL, summarizeTraceLine } from '../lib/mcpTrace';

const RECENT_MS = 6_000;

/**
 * Lightweight live feed for the status bar — tracks last activity text while
 * MCP tools are called from Cursor (or anywhere writing mcp-verbose.log).
 */
export function useMcpTraceStatus() {
  const [connected, setConnected] = useState(false);
  const [info, setInfo] = useState('idle');
  const [recent, setRecent] = useState(false);
  const recentTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let disposed = false;
    let es: EventSource | null = null;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

    function markRecent() {
      setRecent(true);
      if (recentTimer.current) clearTimeout(recentTimer.current);
      recentTimer.current = setTimeout(() => {
        if (!disposed) setRecent(false);
      }, RECENT_MS);
    }

    function connect() {
      if (disposed) return;
      es = new EventSource(MCP_TRACE_EVENTS_URL);
      es.addEventListener('ready', () => {
        if (!disposed) setConnected(true);
      });
      es.addEventListener('line', (ev) => {
        const line = (ev as MessageEvent).data as string;
        if (!line) return;
        setInfo(summarizeTraceLine(line));
        markRecent();
        setConnected(true);
      });
      es.addEventListener('reset', () => {
        if (!disposed) setInfo('cleared');
      });
      es.onerror = () => {
        setConnected(false);
        es?.close();
        es = null;
        if (!disposed) {
          reconnectTimer = setTimeout(connect, 1500);
        }
      };
    }

    connect();
    return () => {
      disposed = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      if (recentTimer.current) clearTimeout(recentTimer.current);
      es?.close();
    };
  }, []);

  return { connected, info, recent };
}
