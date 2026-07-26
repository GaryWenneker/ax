import { useCallback, useEffect, useState } from 'react';
import { createPortal } from 'react-dom';

import Codicon from './Codicon';
import {
  emptyQualitySnapshot,
  fetchMcpQuality,
  formatQualityFixpack,
  gradeTone,
  MCP_QUALITY_EVENTS_URL,
  MCP_QUALITY_FINDING,
  MCP_QUALITY_OPEN,
  openMcpQualitySlideout,
  runMcpAudit,
  type QualityFinding,
  type QualitySnapshot,
} from '../lib/mcpQuality';
import { navigateRoute } from '../lib/routes';
import { WORKSPACE_SWITCHED } from '../workspaceEvents';

type Props = {
  open: boolean;
  onClose: () => void;
  highlightFindingId?: string | null;
};

function fmtPct(n: number) {
  return `${Math.round(n)}%`;
}

function fmtTokens(n: number) {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

function fmtUpdated(ms: number) {
  if (!ms) return '—';
  const diff = Date.now() - ms;
  if (diff < 15_000) return 'just now';
  if (diff < 60_000) return `${Math.floor(diff / 1000)}s ago`;
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  return new Date(ms).toLocaleTimeString();
}

function FindingRow({
  finding,
  active,
  onClick,
}: {
  finding: QualityFinding;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={`mcp-q-finding mcp-q-finding--${finding.severity}${active ? ' mcp-q-finding--active' : ''}`}
      onClick={onClick}
    >
      <div className="mcp-q-finding-top">
        <span className={`mcp-q-sev mcp-q-sev--${finding.severity}`}>{finding.severity}</span>
        <span className="mcp-q-finding-check">{finding.check}</span>
        {finding.tokensEst > 0 && (
          <span className="mcp-q-finding-tokens">~{fmtTokens(finding.tokensEst)} tok</span>
        )}
      </div>
      <div className="mcp-q-finding-title">{finding.title}</div>
      <p className="mcp-q-finding-detail">{finding.detail}</p>
      <p className="mcp-q-finding-waste">{finding.wasteHint}</p>
    </button>
  );
}

export default function McpQualitySlideout({ open, onClose, highlightFindingId }: Props) {
  const [snap, setSnap] = useState<QualitySnapshot>(emptyQualitySnapshot);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [fixpackCopied, setFixpackCopied] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const s = await fetchMcpQuality();
      setSnap(s);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    void refresh();
    const es = new EventSource(MCP_QUALITY_EVENTS_URL);
    es.addEventListener('quality', (ev) => {
      try {
        const data = JSON.parse((ev as MessageEvent).data) as QualitySnapshot;
        setSnap(data);
        setError(null);
      } catch {
        /* ignore */
      }
    });
    es.onerror = () => {
      /* poll fallback */
    };
    const poll = window.setInterval(() => {
      void refresh();
    }, 8_000);
    return () => {
      es.close();
      clearInterval(poll);
    };
  }, [open, refresh]);

  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    const prev = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.removeEventListener('keydown', onKey);
      document.body.style.overflow = prev;
    };
  }, [open, onClose]);

  useEffect(() => {
    function onWs() {
      void refresh();
    }
    window.addEventListener(WORKSPACE_SWITCHED, onWs);
    return () => window.removeEventListener(WORKSPACE_SWITCHED, onWs);
  }, [refresh]);

  async function runFullAudit() {
    setBusy(true);
    try {
      const s = await runMcpAudit({ markdown: false });
      setSnap(s);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function copyFixpack() {
    const text = formatQualityFixpack(snap);
    try {
      await navigator.clipboard.writeText(text);
      setFixpackCopied(true);
      window.setTimeout(() => setFixpackCopied(false), 2000);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not copy fixpack');
    }
  }

  function onFinding(f: QualityFinding) {
    window.dispatchEvent(
      new CustomEvent(MCP_QUALITY_FINDING, {
        detail: { tool: f.tool, findingId: f.id, logLineHint: f.logLineHint },
      }),
    );
    navigateRoute({ page: 'logging' });
  }

  if (!open) return null;

  const tone = gradeTone(snap.score);

  return createPortal(
    <div className="mcp-q-overlay" role="presentation">
      <button type="button" className="mcp-q-backdrop" aria-label="Close quality panel" onClick={onClose} />
      <aside className="mcp-q-blade detail-panel" role="dialog" aria-label="MCP quality metrics">
        <header className="detail-header mcp-q-header">
          <div className="mcp-q-header-main">
            <div className="detail-title">MCP quality</div>
            <div className="mcp-q-header-sub">
              {snap.projectLabel || '—'} · last {snap.windowMinutes}m · {fmtUpdated(snap.updatedAtMs)}
            </div>
          </div>
          <div className={`mcp-q-score mcp-q-score--${tone}`}>
            <span className="mcp-q-score-num">{snap.score || '—'}</span>
            <span className="mcp-q-score-grade">{snap.grade}</span>
          </div>
          <button type="button" className="detail-close" aria-label="Close" onClick={onClose}>
            <Codicon name="close" />
          </button>
        </header>

        <div className="mcp-q-body">
          {error && <p className="mcp-q-error">{error}</p>}

          {!snap.verboseEnabled && (
            <div className="mcp-q-banner mcp-q-banner--warn">
              Verbose MCP logging is off. Enable it in Settings → Interface so the quality loop can score enrichment.
            </div>
          )}

          <section className="mcp-q-section">
            <h3 className="mcp-q-section-title">Correlation</h3>
            <div className="mcp-q-grid">
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">Match</span>
                <span className="mcp-q-metric-val">{fmtPct(snap.correlationPct)}</span>
              </div>
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">Matched</span>
                <span className="mcp-q-metric-val">{snap.matchedCalls}</span>
              </div>
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">Unmatched</span>
                <span className="mcp-q-metric-val">{snap.unmatchedAxCalls}</span>
              </div>
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">Mode</span>
                <span className="mcp-q-metric-val mono">{snap.mode}</span>
              </div>
            </div>
            {snap.logPath && (
              <p className="mcp-q-muted mono" title={snap.logPath}>
                {snap.logPath}
              </p>
            )}
          </section>

          <section className="mcp-q-section">
            <h3 className="mcp-q-section-title">Enrichment</h3>
            <div className="mcp-q-grid">
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">inject p50</span>
                <span className="mcp-q-metric-val">{snap.enrichment.injectCharsP50.toLocaleString()}</span>
              </div>
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">inject p95</span>
                <span className="mcp-q-metric-val">{snap.enrichment.injectCharsP95.toLocaleString()}</span>
              </div>
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">enrich done</span>
                <span className="mcp-q-metric-val">{fmtPct(snap.enrichment.enrichDoneRate * 100)}</span>
              </div>
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">empty</span>
                <span className="mcp-q-metric-val">{snap.enrichment.emptyEnrichCount}</span>
              </div>
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">rules rate</span>
                <span className="mcp-q-metric-val">{fmtPct(snap.enrichment.matchedRulesRate * 100)}</span>
              </div>
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">preflight</span>
                <span className="mcp-q-metric-val">
                  {snap.enrichment.preflightCount}/{snap.enrichment.inboundCount}
                </span>
              </div>
            </div>
          </section>

          <section className="mcp-q-section">
            <h3 className="mcp-q-section-title">Tool mix</h3>
            <div className="mcp-q-grid mcp-q-grid--wide">
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">preflight</span>
                <span className="mcp-q-metric-val">{snap.toolMix.preflight}</span>
              </div>
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">explore</span>
                <span className="mcp-q-metric-val">{snap.toolMix.explore}</span>
              </div>
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">guard</span>
                <span className="mcp-q-metric-val">{snap.toolMix.guard}</span>
              </div>
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">graph</span>
                <span className="mcp-q-metric-val">{snap.toolMix.graph}</span>
              </div>
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">Read</span>
                <span className="mcp-q-metric-val">{snap.toolMix.read}</span>
              </div>
              <div className="mcp-q-metric">
                <span className="mcp-q-metric-label">Grep</span>
                <span className="mcp-q-metric-val">{snap.toolMix.grep}</span>
              </div>
            </div>
          </section>

          <section className="mcp-q-section">
            <h3 className="mcp-q-section-title">Token waste</h3>
            <p className="mcp-q-waste">
              ~{fmtTokens(snap.tokensAtRisk)} tokens at risk this window
              {snap.criticalCount > 0 ? ` · ${snap.criticalCount} critical` : ''}
            </p>
            <button
              type="button"
              className="mcp-q-link"
              onClick={() => {
                onClose();
                navigateRoute({ page: 'savings' });
              }}
            >
              Open Savings →
            </button>
          </section>

          <section className="mcp-q-section">
            <h3 className="mcp-q-section-title">Findings ({snap.findings.length})</h3>
            {snap.findings.length === 0 ? (
              <p className="mcp-q-muted">No findings in this window.</p>
            ) : (
              <div className="mcp-q-findings">
                {snap.findings.map((f) => (
                  <FindingRow
                    key={f.id}
                    finding={f}
                    active={highlightFindingId === f.id}
                    onClick={() => onFinding(f)}
                  />
                ))}
              </div>
            )}
          </section>

          <section className="mcp-q-section mcp-q-actions">
            <button
              type="button"
              className="btn primary"
              title="Copy an agent-ready Markdown fixpack from these findings"
              onClick={() => void copyFixpack()}
            >
              {fixpackCopied ? 'Copied fixpack' : 'Copy fixpack'}
            </button>
            <button type="button" className="btn" disabled={busy} onClick={() => void runFullAudit()}>
              {busy ? 'Running…' : 'Run full session audit'}
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => {
                onClose();
                navigateRoute({ page: 'logging' });
              }}
            >
              Open Logging
            </button>
            <button type="button" className="btn" onClick={() => void refresh()}>
              Refresh
            </button>
          </section>
        </div>
      </aside>
    </div>,
    document.body,
  );
}

/** Compact strip used on Logging — opens the same slide-out. */
export function McpQualityStrip({ snap }: { snap: QualitySnapshot | null }) {
  const s = snap ?? emptyQualitySnapshot();
  const tone = gradeTone(s.score);
  return (
    <button
      type="button"
      className={`mcp-q-strip mcp-q-strip--${tone}`}
      onClick={() => openMcpQualitySlideout()}
      title="Open MCP quality metrics"
    >
      <span className="mcp-q-strip-label">Quality</span>
      <span className="mcp-q-strip-score">
        {s.score || '—'} {s.grade !== '—' ? s.grade : ''}
      </span>
      <span className="mcp-q-strip-meta">corr {fmtPct(s.correlationPct)}</span>
      {s.criticalCount > 0 && <span className="mcp-q-strip-badge">{s.criticalCount}</span>}
      <span className="mcp-q-strip-meta">~{fmtTokens(s.tokensAtRisk)} risk</span>
    </button>
  );
}

/** Host that owns open state for StatusBar + Logging strip. */
export function McpQualityHost() {
  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState<string | null>(null);

  useEffect(() => {
    function onOpen(ev: Event) {
      const detail = (ev as CustomEvent<{ findingId?: string }>).detail;
      setHighlight(detail?.findingId ?? null);
      setOpen(true);
    }
    window.addEventListener(MCP_QUALITY_OPEN, onOpen);
    return () => window.removeEventListener(MCP_QUALITY_OPEN, onOpen);
  }, []);

  return (
    <McpQualitySlideout
      open={open}
      highlightFindingId={highlight}
      onClose={() => {
        setOpen(false);
        setHighlight(null);
      }}
    />
  );
}
