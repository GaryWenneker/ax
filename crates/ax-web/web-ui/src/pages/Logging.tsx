import { useEffect, useState } from 'react';

import McpTraceLive from '../components/McpTraceLive';
import { usePageContext } from '../context/UiContext';
import {
  emptyMcpTraceStats,
  MCP_TRACE_STATS,
  type McpTraceStats,
} from '../lib/mcpTraceEvents';
import { navigateRoute } from '../lib/routes';
import { fetchShipConfig } from '../shipApi';

/**
 * Full-page real-time MCP verbose log for <project>/.ax/mcp-verbose-*.log
 * (newest at top). Opens maximized above the status bar (logging stats +
 * project switch live there). Enable/disable lives on Settings → Interface.
 */
export default function LoggingPage() {
  const [verboseEnabled, setVerboseEnabled] = useState(false);
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

  useEffect(() => {
    let cancelled = false;
    fetchShipConfig()
      .then((d) => {
        if (!cancelled) setVerboseEnabled(d.config.ui?.verbose_mcp ?? false);
      })
      .catch(() => {});
    function onConfig(ev: Event) {
      const detail = (ev as CustomEvent<{ verbose_mcp?: boolean }>).detail;
      if (typeof detail?.verbose_mcp === 'boolean') {
        setVerboseEnabled(detail.verbose_mcp);
      } else {
        fetchShipConfig()
          .then((d) => setVerboseEnabled(d.config.ui?.verbose_mcp ?? false))
          .catch(() => {});
      }
    }
    window.addEventListener('ax-ship-config-updated', onConfig);
    return () => {
      cancelled = true;
      window.removeEventListener('ax-ship-config-updated', onConfig);
    };
  }, []);

  return (
    <div className="logging-page logging-page--immersive">
      {!verboseEnabled && (
        <div className="logging-verbose-bar">
          <div className="logging-verbose-bar-copy">
            <span className="logging-verbose-bar-title">Verbose MCP logging is off</span>
            <span className="logging-verbose-bar-desc">
              Enable it under{' '}
              <button
                type="button"
                className="logging-verbose-settings-link"
                onClick={() => navigateRoute({ page: 'settings' })}
              >
                Settings → Interface
              </button>
              . Reconnect ax MCP after enabling. History below still tails the project log file.
            </span>
          </div>
        </div>
      )}
      <McpTraceLive verboseEnabled={verboseEnabled} variant="page" />
    </div>
  );
}
