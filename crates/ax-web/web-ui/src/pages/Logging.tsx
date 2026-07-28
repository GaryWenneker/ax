import { useEffect, useState } from 'react';

import McpTraceLive from '../components/McpTraceLive';
import { usePageContext } from '../context/UiContext';
import {
  emptyMcpTraceStats,
  MCP_TRACE_STATS,
  type McpTraceStats,
} from '../lib/mcpTraceEvents';
import { fetchShipConfig, saveShipConfig } from '../shipApi';

/**
 * Full-page real-time MCP verbose log for <project>/.ax/mcp-verbose-*.log
 * (newest at top). Opens maximized above the status bar (logging stats +
 * project switch live there).
 */
export default function LoggingPage() {
  const [verboseEnabled, setVerboseEnabled] = useState(false);
  const [verboseBusy, setVerboseBusy] = useState(false);
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

  async function setVerboseMcp(verbose_mcp: boolean) {
    setVerboseBusy(true);
    setVerboseEnabled(verbose_mcp);
    try {
      const d = await fetchShipConfig();
      await saveShipConfig({
        ...d.config,
        ui: { ...(d.config.ui ?? {}), verbose_mcp },
      });
      window.dispatchEvent(
        new CustomEvent('ax-ship-config-updated', { detail: { verbose_mcp } }),
      );
    } catch {
      setVerboseEnabled(!verbose_mcp);
    } finally {
      setVerboseBusy(false);
    }
  }

  return (
    <div className="logging-page logging-page--immersive">
      <div className="logging-verbose-bar">
        <button
          type="button"
          role="switch"
          aria-checked={verboseEnabled}
          aria-label="Verbose MCP logging"
          disabled={verboseBusy}
          className={`settings-toggle${verboseEnabled ? ' on' : ''}${verboseBusy ? ' disabled' : ''}`}
          onClick={() => void setVerboseMcp(!verboseEnabled)}
        >
          <span className="settings-toggle-thumb" />
        </button>
        <div className="logging-verbose-bar-copy">
          <span className="logging-verbose-bar-title">Verbose MCP logging</span>
          <span className="logging-verbose-bar-desc">
            Record tool calls to{' '}
            <code>.ax/mcp-verbose-YYYY-MM-DD.log</code>. Reconnect ax MCP after
            enabling. Off by default — never alters tool responses.
          </span>
        </div>
      </div>
      <McpTraceLive verboseEnabled={verboseEnabled} variant="page" />
    </div>
  );
}
