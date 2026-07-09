import { useEffect, useState } from 'react';

import { usePageContext } from '../context/UiContext';
import { runShipCommand, type GateStep, type ShipReport } from '../shipApi';

interface Props {
  onOpenSettings: () => void;
}

const PIPELINE_STEPS = ['index', 'tia', 'tests', 'sonar', 'policy'];

function stepIcon(status: string, active: boolean) {
  if (active) return '◌';
  if (status === 'passed') return '✓';
  if (status === 'failed') return '✕';
  return '·';
}

export default function ShipPage({ onOpenSettings }: Props) {
  const [report, setReport] = useState<ShipReport | null>(null);
  const [branch, setBranch] = useState<string | null>(null);
  const [connected, setConnected] = useState(false);
  const [liveStep, setLiveStep] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const qg = report?.quality_gate;
  const passed = qg?.passed;
  const testCount = report?.tia?.tests.length ?? 0;
  const changedCount = report?.changed_files?.length ?? 0;

  usePageContext(
    'Command Center',
    qg ? (passed ? 'Quality gate passed' : 'Quality gate failed') : undefined,
  );

  useEffect(() => {
    fetch('/api/ship/status')
      .then((r) => r.json())
      .then((d) => {
        setBranch(d.branch ?? null);
        if (d.report) setReport(d.report);
      })
      .catch(() => {});

    const es = new EventSource('/api/ship/events');
    es.onopen = () => setConnected(true);
    es.onerror = () => setConnected(false);
    es.onmessage = (ev) => {
      try {
        const payload = JSON.parse(ev.data);
        if (payload.type === 'step_started') setLiveStep(payload.step ?? null);
        if (payload.type === 'step_finished') setLiveStep(null);
        if (payload.type === 'report_updated' && payload.report) setReport(payload.report);
        if (payload.type === 'git_changed' && payload.branch) setBranch(payload.branch);
      } catch {
        /* ignore */
      }
    };
    return () => es.close();
  }, []);

  async function runCommand(cmd: string) {
    setBusy(cmd);
    setErr(null);
    setMsg(null);
    try {
      const r = await runShipCommand(cmd);
      if (!r.ok) throw new Error(r.error ?? 'Command failed');
      if (r.report) setReport(r.report);
      if (r.pr) setMsg(`Draft PR #${r.pr.number} — ${r.pr.url}`);
      else setMsg(cmd === 'evaluate' ? 'Evaluation complete' : 'Done');
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  }

  const stepsByName = new Map(qg?.steps?.map((s) => [s.step, s]) ?? []);

  return (
    <div className="page ship-page">
      <div className="page-header">
        <div>
          <h1 className="page-title">Command Center</h1>
          <p className="ship-subtitle muted">
            Git-aware quality gate · live pipeline via SSE
          </p>
        </div>
        <div className="page-actions">
          <button type="button" className="btn" disabled={!!busy} onClick={onOpenSettings}>
            Settings
          </button>
          <button type="button" className="btn primary" disabled={!!busy} onClick={() => runCommand('evaluate')}>
            {busy === 'evaluate' ? 'Evaluating…' : 'Evaluate'}
          </button>
          <button type="button" className="btn" disabled={!!busy} onClick={() => runCommand('draft')}>
            {busy === 'draft' ? 'Creating…' : 'Draft PR'}
          </button>
        </div>
      </div>

      {msg && <div className="ship-banner ship-banner--ok">{msg}</div>}
      {err && <div className="ship-banner ship-banner--err">{err}</div>}

      <div className="ship-metrics">
        <div className="ship-metric">
          <span className="ship-metric-label">Branch</span>
          <span className="ship-metric-value">{branch ?? '—'}</span>
        </div>
        <div className="ship-metric">
          <span className="ship-metric-label">Live feed</span>
          <span className={`ship-metric-value ${connected ? 'ok' : 'warn'}`}>
            {connected ? 'connected' : 'offline'}
          </span>
        </div>
        <div className="ship-metric">
          <span className="ship-metric-label">Changed files</span>
          <span className="ship-metric-value">{changedCount}</span>
        </div>
        <div className="ship-metric">
          <span className="ship-metric-label">Impacted tests</span>
          <span className="ship-metric-value">{testCount}</span>
        </div>
        <div className="ship-metric">
          <span className="ship-metric-label">Sonar</span>
          <span className="ship-metric-value">
            {qg?.sonar?.status ?? '—'}
          </span>
        </div>
      </div>

      <div className={`ship-gate ${passed === true ? 'ship-gate--pass' : passed === false ? 'ship-gate--fail' : ''}`}>
        <div className="ship-gate-status">
          <span className="ship-gate-label">Quality gate</span>
          <strong className="ship-gate-title">
            {!qg ? 'Not evaluated yet' : passed ? 'All checks passed' : 'Checks failed'}
          </strong>
          {liveStep && <span className="badge ship-live-badge">Running {liveStep}</span>}
        </div>
      </div>

      <div className="detail-section-title" style={{ marginBottom: '8px' }}>Pipeline</div>
      <div className="ship-pipeline">
        {PIPELINE_STEPS.map((name) => {
          const s = stepsByName.get(name) as GateStep | undefined;
          const status = s?.status ?? 'pending';
          const active = liveStep === name;
          return (
            <div
              key={name}
              className={`ship-step ship-step--${status}${active ? ' ship-step--active' : ''}`}
            >
              <div className="ship-step-icon" aria-hidden="true">
                {stepIcon(status, active)}
              </div>
              <div className="ship-step-body">
                <div className="ship-step-name">{name}</div>
                <div className="ship-step-meta muted">
                  {s?.detail ?? (active ? 'running…' : status)}
                </div>
              </div>
            </div>
          );
        })}
      </div>

      {report?.changed_files && report.changed_files.length > 0 && (
        <>
          <div className="detail-section-title" style={{ margin: '16px 0 8px' }}>Changed files</div>
          <ul className="ship-file-list">
            {report.changed_files.map((f) => (
              <li key={f}><code>{f}</code></li>
            ))}
          </ul>
        </>
      )}

      {report?.tia && report.tia.tests.length > 0 && (
        <>
          <div className="detail-section-title" style={{ margin: '16px 0 8px' }}>Test impact</div>
          <ul className="ship-file-list">
            {report.tia.tests.map((t) => (
              <li key={t.name}>
                <code>{t.name}</code>
                <span className="muted"> · {t.runner_hint}</span>
              </li>
            ))}
          </ul>
        </>
      )}

      {(report?.breaking_warnings?.length ?? 0) > 0 && (
        <div className="ship-alert ship-alert--warn">
          <div className="detail-section-title">Breaking changes</div>
          <ul>
            {report!.breaking_warnings!.map((w, i) => (
              <li key={i}>{w.node_name}: {w.reason}</li>
            ))}
          </ul>
        </div>
      )}

      {(report?.business_rule_warnings?.length ?? 0) > 0 && (
        <div className="ship-alert ship-alert--warn">
          <div className="detail-section-title">Business rules</div>
          <ul>
            {report!.business_rule_warnings!.map((w, i) => (
              <li key={i}>[{w.severity}] {w.rule_text}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
