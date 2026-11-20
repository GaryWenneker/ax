import { useCallback, useEffect, useState } from 'react';

import ContinuityPanel from '../components/ContinuityPanel';
import McpTraceLive from '../components/McpTraceLive';
import { usePageContext } from '../context/UiContext';
import type { ContinuityDraft } from '../lib/continuity';
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
  const [continuityOpen, setContinuityOpen] = useState(false);
  const [pickedContinuity, setPickedContinuity] = useState<ContinuityDraft | null>(null);
  const onContinuityOpen = useCallback((open: boolean) => {
    setContinuityOpen(open);
    if (!open) setPickedContinuity(null);
  }, []);

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
      <ContinuityPanel values={pickedContinuity ?? stats.continuity} onOpenChange={onContinuityOpen} />
      <McpTraceLive variant="page" rowOpensBlade={!continuityOpen} onContinuityRow={setPickedContinuity} />
    </div>
  );
}
