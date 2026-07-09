import { useEffect, useState } from 'react';

import {
  PageCard,
  PageCardBody,
  PageHero,
  PageShell,
  PageStack,
  PageToasts,
  StatusPanel,
  StatusPill,
} from '../components/ui/PageLayout';
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
    <PageShell>
      <PageHero
        title="Command Center"
        subtitle="Git-aware quality gate with live pipeline updates via SSE."
        actions={
          <>
            <button type="button" className="btn" disabled={!!busy} onClick={onOpenSettings}>
              Settings
            </button>
            <button type="button" className="btn primary" disabled={!!busy} onClick={() => runCommand('evaluate')}>
              {busy === 'evaluate' ? 'Evaluating…' : 'Evaluate'}
            </button>
            <button type="button" className="btn" disabled={!!busy} onClick={() => runCommand('draft')}>
              {busy === 'draft' ? 'Creating…' : 'Draft PR'}
            </button>
          </>
        }
      />

      <PageToasts ok={msg} err={err} />

      <PageStack>
        <PageCard title="Overview" description="Current branch and pipeline status.">
          <StatusPanel title="Metrics">
            <StatusPill label="Branch" value={branch ?? '—'} />
            <StatusPill
              label="Live feed"
              value={connected ? 'connected' : 'offline'}
              tone={connected ? 'ok' : 'warn'}
            />
            <StatusPill label="Changed files" value={String(changedCount)} />
            <StatusPill label="Impacted tests" value={String(testCount)} />
            <StatusPill label="Sonar" value={qg?.sonar?.status ?? '—'} />
          </StatusPanel>
        </PageCard>

        <PageCard
          title="Quality gate"
          description={
            !qg
              ? 'Run Evaluate to check index, tests, Sonar, and policy.'
              : passed
                ? 'All pipeline checks passed.'
                : 'One or more checks failed.'
          }
        >
          <PageCardBody>
            <div
              className={`ship-gate${passed === true ? ' ship-gate--pass' : passed === false ? ' ship-gate--fail' : ''}`}
              style={{ margin: '0 clamp(16px, 2vw, 28px) 14px', borderRadius: 8 }}
            >
              <div className="ship-gate-status">
                <span className="ship-gate-label">Status</span>
                <strong className="ship-gate-title">
                  {!qg ? 'Not evaluated yet' : passed ? 'All checks passed' : 'Checks failed'}
                </strong>
                {liveStep && <span className="badge ship-live-badge">Running {liveStep}</span>}
              </div>
            </div>
          </PageCardBody>
        </PageCard>

        <PageCard title="Pipeline" description="Step-by-step quality gate progress.">
          <div className="ship-pipeline-grid">
            {PIPELINE_STEPS.map((name) => {
              const s = stepsByName.get(name) as GateStep | undefined;
              const status = s?.status ?? 'pending';
              const active = liveStep === name;
              return (
                <div
                  key={name}
                  className={`ship-pipeline-step ship-pipeline-step--${status}${active ? ' ship-pipeline-step--active' : ''}`}
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
        </PageCard>

        {report?.changed_files && report.changed_files.length > 0 && (
          <PageCard title="Changed files" description={`${report.changed_files.length} files in the diff.`}>
            <PageCardBody>
              <ul className="ship-file-list" style={{ border: 'none' }}>
                {report.changed_files.map((f) => (
                  <li key={f} style={{ padding: '8px clamp(16px, 2vw, 28px)' }}>
                    <code>{f}</code>
                  </li>
                ))}
              </ul>
            </PageCardBody>
          </PageCard>
        )}

        {report?.tia && report.tia.tests.length > 0 && (
          <PageCard title="Test impact" description={`${report.tia.tests.length} impacted tests.`}>
            <PageCardBody>
              <ul className="ship-file-list" style={{ border: 'none' }}>
                {report.tia.tests.map((t) => (
                  <li key={t.name} style={{ padding: '8px clamp(16px, 2vw, 28px)' }}>
                    <code>{t.name}</code>
                    <span className="muted"> · {t.runner_hint}</span>
                  </li>
                ))}
              </ul>
            </PageCardBody>
          </PageCard>
        )}

        {(report?.breaking_warnings?.length ?? 0) > 0 && (
          <PageCard title="Breaking changes" description="API changes detected in the diff.">
            <PageCardBody>
              <div className="ship-alert ship-alert--warn" style={{ margin: '0 clamp(16px, 2vw, 28px) 14px', borderRadius: 8 }}>
                <ul>
                  {report!.breaking_warnings!.map((w, i) => (
                    <li key={i}>{w.node_name}: {w.reason}</li>
                  ))}
                </ul>
              </div>
            </PageCardBody>
          </PageCard>
        )}

        {(report?.business_rule_warnings?.length ?? 0) > 0 && (
          <PageCard title="Business rules" description="Policy warnings from business rule checks.">
            <PageCardBody>
              <div className="ship-alert ship-alert--warn" style={{ margin: '0 clamp(16px, 2vw, 28px) 14px', borderRadius: 8 }}>
                <ul>
                  {report!.business_rule_warnings!.map((w, i) => (
                    <li key={i}>[{w.severity}] {w.rule_text}</li>
                  ))}
                </ul>
              </div>
            </PageCardBody>
          </PageCard>
        )}
      </PageStack>
    </PageShell>
  );
}
