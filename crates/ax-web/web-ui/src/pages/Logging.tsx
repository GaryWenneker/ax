import { useEffect, useState } from 'react';

import McpTraceLive from '../components/McpTraceLive';
import { usePageContext } from '../context/UiContext';
import {
  emptyMcpTraceStats,
  MCP_TRACE_STATS,
  type McpTraceStats,
} from '../lib/mcpTraceEvents';

/**
 * Transport-agnostic activity log for <project>/.ax/mcp-verbose-*.log
 * (MCP, CLI, hooks, agent-reported). Newest at top. Verbose logging is always on.
 * Filter by kind (CLI / MCP inbound / Hook / Agent) in the live toolbar.
 */
export default function LoggingPage() {
  const [stats, setStats] = useState<McpTraceStats>(emptyMcpTraceStats);

  const detail =
    stats.projectLabel && stats.projectLabel !== '—'
      ? `${stats.projectLabel} · ${stats.total.toLocaleString()} events · ${
          stats.live ? 'live' : 'offline'
        }`
      : 'Activity · live stream';

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
