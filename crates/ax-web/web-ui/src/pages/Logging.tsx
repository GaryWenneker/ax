import { useEffect, useState } from 'react';

import McpTraceLive from '../components/McpTraceLive';
import { usePageContext } from '../context/UiContext';
import {
  emptyMcpTraceStats,
  MCP_TRACE_STATS,
  type McpTraceStats,
} from '../lib/mcpTraceEvents';

/**
 * Full-page real-time MCP verbose log for <project>/.ax/mcp-verbose-*.log
 * (newest at top). Stays in the app chrome; Full is a button, not the default.
 * Verbose MCP logging is always on.
 */
export default function LoggingPage() {
  const [stats, setStats] = useState<McpTraceStats>(emptyMcpTraceStats);

  const detail =
    stats.projectLabel && stats.projectLabel !== '—'
      ? `${stats.projectLabel} · ${stats.total.toLocaleString()} events · ${
          stats.live ? 'live' : 'offline'
        }`
      : 'MCP verbose · live stream';

  usePageContext('Logging', detail);

  useEffect(() => {
    function onStats(ev: Event) {
      const detail = (ev as CustomEvent<McpTraceStats>).detail;
      if (detail) setStats(detail);
    }
    window.addEventListener(MCP_TRACE_STATS, onStats);
    return () => window.removeEventListener(MCP_TRACE_STATS, onStats);
  }, []);

  return (
    <div className="logging-page">
      <McpTraceLive variant="page" />
    </div>
  );
}
