import { useEffect, useState } from 'react';

interface GateStep {
  step: string;
  status: string;
  detail?: string;
}

interface ShipReport {
  git?: { head_branch?: string; base_ref: string };
  changed_files?: string[];
  tia?: { tests: Array<{ name: string; runner_hint: string }>; test_files: string[] };
  quality_gate?: { passed: boolean; steps: GateStep[]; sonar?: { status: string; passed: boolean } };
  breaking_warnings?: Array<{ node_name: string; reason: string }>;
  business_rule_warnings?: Array<{ rule_text: string; severity: string }>;
  affected_routes?: string[];
}

export default function ShipPage() {
  const [report, setReport] = useState<ShipReport | null>(null);
  const [branch, setBranch] = useState<string | null>(null);
  const [connected, setConnected] = useState(false);
  const [log, setLog] = useState<string[]>([]);

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
        const msg = JSON.parse(ev.data);
        setLog((prev) => [`${msg.type ?? 'event'}`, ...prev].slice(0, 20));
        if (msg.type === 'report_updated' || msg.ReportUpdated) {
          setReport(msg.ReportUpdated ?? msg);
        }
        if (msg.type === 'git_changed' && msg.branch) {
          setBranch(msg.branch);
        }
      } catch {
        /* ignore */
      }
    };
    return () => es.close();
  }, []);

  async function runCommand(cmd: string) {
    await fetch('/api/ship/command', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ cmd }),
    });
  }

  const qg = report?.quality_gate;

  return (
    <div className="page ship-page">
      <h1>Command Center</h1>
      <p className="muted">
        Branch: <strong>{branch ?? '—'}</strong>
        {' · '}
        SSE: {connected ? 'live' : 'disconnected'}
      </p>

      <div className="ship-actions">
        <button type="button" onClick={() => runCommand('evaluate')}>Evaluate</button>
        <button type="button" onClick={() => runCommand('draft')}>Draft PR</button>
      </div>

      <section className="panel">
        <h2>Quality Gate</h2>
        <p className={qg?.passed ? 'ok' : 'warn'}>
          {qg?.passed ? 'Passed' : qg ? 'Failed / pending' : 'No evaluation yet'}
        </p>
        <ul>
          {qg?.steps?.map((s) => (
            <li key={s.step}>
              [{s.status}] {s.step} {s.detail ? `— ${s.detail}` : ''}
            </li>
          ))}
        </ul>
        {qg?.sonar && (
          <p>SonarQube: {qg.sonar.status} ({qg.sonar.passed ? 'ok' : 'fail'})</p>
        )}
      </section>

      {report?.tia && (
        <section className="panel">
          <h2>Test Impact ({report.tia.tests.length})</h2>
          <ul>
            {report.tia.tests.map((t) => (
              <li key={t.name}>
                <code>{t.name}</code> — {t.runner_hint}
              </li>
            ))}
          </ul>
        </section>
      )}

      {report?.affected_routes && report.affected_routes.length > 0 && (
        <section className="panel">
          <h2>Affected Routes</h2>
          <ul>{report.affected_routes.map((r) => <li key={r}>{r}</li>)}</ul>
        </section>
      )}

      {report?.breaking_warnings && report.breaking_warnings.length > 0 && (
        <section className="panel warn">
          <h2>Breaking Change Warnings</h2>
          <ul>
            {report.breaking_warnings.map((w, i) => (
              <li key={i}>{w.node_name}: {w.reason}</li>
            ))}
          </ul>
        </section>
      )}

      {report?.business_rule_warnings && report.business_rule_warnings.length > 0 && (
        <section className="panel warn">
          <h2>Business Rules</h2>
          <ul>
            {report.business_rule_warnings.map((w, i) => (
              <li key={i}>[{w.severity}] {w.rule_text}</li>
            ))}
          </ul>
        </section>
      )}

      <section className="panel">
        <h2>Event log</h2>
        <pre className="event-log">{log.join('\n')}</pre>
      </section>
    </div>
  );
}
