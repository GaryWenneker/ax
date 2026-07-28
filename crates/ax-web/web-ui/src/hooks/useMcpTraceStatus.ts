import { useEffect, useRef, useState } from 'react';

import {
  MCP_TRACE_ACTIVITY,
  MCP_TRACE_STATS,
  type McpTraceStats,
} from '../lib/mcpTraceEvents';

const RECENT_MS = 6_000;

/**
 * Status-bar MCP activity indicator.
 *
 * Does **not** open its own SSE — that used to hold a permanent socket on every
 * page and (with multiple tabs) exhaust the browser's ~6 HTTP/1.1 connections
 * per host so `/api/*` fetches hung. Logging's `McpTraceLive` owns the stream and
 * publishes stats/activity via window events.
 */
export function useMcpTraceStatus() {
  const [connected, setConnected] = useState(false);
  const [info, setInfo] = useState('idle');
  const [recent, setRecent] = useState(false);
  const recentTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let disposed = false;

    function markRecent() {
      setRecent(true);
      if (recentTimer.current) clearTimeout(recentTimer.current);
      recentTimer.current = setTimeout(() => {
        if (!disposed) setRecent(false);
      }, RECENT_MS);
    }

    function onStats(ev: Event) {
      const detail = (ev as CustomEvent<McpTraceStats>).detail;
      if (!detail || disposed) return;
      setConnected(detail.live);
      setInfo((prev) => {
        if (!detail.live) return 'offline';
        if (prev === 'offline' || prev === 'idle') return 'listening';
        return prev;
      });
    }

    function onActivity(ev: Event) {
      const detail = (ev as CustomEvent<{ summary?: string }>).detail;
      if (!detail?.summary || disposed) return;
      setInfo(detail.summary);
      setConnected(true);
      markRecent();
    }

    window.addEventListener(MCP_TRACE_STATS, onStats);
    window.addEventListener(MCP_TRACE_ACTIVITY, onActivity);
    return () => {
      disposed = true;
      if (recentTimer.current) clearTimeout(recentTimer.current);
      window.removeEventListener(MCP_TRACE_STATS, onStats);
      window.removeEventListener(MCP_TRACE_ACTIVITY, onActivity);
    };
  }, []);

  return { connected, info, recent };
}
