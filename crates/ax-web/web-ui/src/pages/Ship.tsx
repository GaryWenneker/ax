import { useCallback, useEffect, useRef, useState } from 'react';

import PipelineTrack from '../components/PipelineTrack';
import {
  BusyLabel,
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
import { subscribeSharedEventSource } from '../lib/sharedEventSource';
import {
  bootstrapSonar,
  discoverSonar,
  fetchShipStatus,
  regenerateSonarToken,
  runShipCommand,
  streamSonarInstall,
  streamSonarScanAll,
  streamSonarScanProject,
  streamSonarStart,
  toggleSonarExclude,
  validateSonarToken,
  type GateStep,
  type LastRunLog,
  type ShipReport,
  type SonarDiscovery,
  type SonarSetupStatus,
  type SonarStreamEvent,
} from '../shipApi';
import {
  isEvaluationInProgress,
  mergePipelineFromLog,
  parsePipelineFromLog,
  seedSonarProjects,
  applySonarProjectEvent,
  asSonarProjectSteps,
  finalizeSonarProjectSteps,
  resolveSonarProjectCardStatus,
  type SonarProjectStep,
} from '../pipelineState';
import { formatLogLine, liveActivityLabel } from '../logFormat';

interface Props {
  onOpenSonar: () => void;
}

function RunLogPanel({
  log,
  active,
  liveStep,
  liveSonarKey,
  sonarProjects,
}: {
  log: LastRunLog | null;
  active: boolean;
  liveStep: string | null;
  liveSonarKey: string | null;
  sonarProjects: SonarProjectStep[];
}) {
  const preRef = useRef<HTMLPreElement>(null);

  useEffect(() => {
    const el = preRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [log?.lines?.length]);

  const formattedLines = (log?.lines ?? []).map(formatLogLine);
  const liveLabel = active
    ? liveActivityLabel(
        log?.lines,
        liveStep,
        liveSonarKey,
        sonarProjects.find((p) => p.key === liveSonarKey)?.name,
      )
    : null;
  const highlightIndex =
    active && formattedLines.length > 0 ? formattedLines.length - 1 : -1;

  if (!log?.lines?.length && !active) return null;
  return (
    <PageCard
      title="Last run log"
      description={
        log?.finished_at
          ? log.ok
            ? 'Last evaluation completed successfully.'
            : 'Last evaluation finished with errors.'
          : active
            ? 'Evaluation in progress…'
            : 'Log from the most recent Evaluate run.'
      }
    >
      <PageCardBody>
        <div className="settings-log-panel" style={{ margin: '0 clamp(16px, 2vw, 28px) 14px' }}>
          <div className="settings-log-header">
            <span>Pipeline log</span>
            {active && (
              <span className="settings-log-live">
                live{liveLabel ? ` · ${liveLabel}` : ''}
              </span>
            )}
            {log?.ok === false && !active && (
              <span className="settings-log-live settings-log-live--err">failed</span>
            )}
          </div>
          <pre ref={preRef} className="settings-log-body" aria-live="polite">
            {formattedLines.length
              ? formattedLines.map((line, i) => (
                  <span
                    key={`${i}-${line}`}
                    className={
                      i === highlightIndex ? 'settings-log-line settings-log-line--live' : 'settings-log-line'
                    }
                  >
                    {line}
                    {'\n'}
                  </span>
                ))
              : 'Waiting for output…'}
          </pre>
        </div>
      </PageCardBody>
    </PageCard>
  );
}

export default function ShipPage({ onOpenSonar }: Props) {
  const [report, setReport] = useState<ShipReport | null>(null);
  const [branch, setBranch] = useState<string | null>(null);
  const [gitRoots, setGitRoots] = useState<string[]>([]);
  const [connected, setConnected] = useState(false);
  const [liveStep, setLiveStep] = useState<string | null>(null);
  const [liveSonarKey, setLiveSonarKey] = useState<string | null>(null);
  const [liveSteps, setLiveSteps] = useState<Map<string, GateStep>>(new Map());
  const [sonarProjectSteps, setSonarProjectSteps] = useState<SonarProjectStep[]>([]);
  const [sonarPhase, setSonarPhase] = useState<'preparing' | 'scanning' | null>(null);
  const [lastRun, setLastRun] = useState<LastRunLog | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [sonarDiscovery, setSonarDiscovery] = useState<SonarDiscovery | null>(null);
  const [sonarSetup, setSonarSetup] = useState<SonarSetupStatus | null>(null);
  const [sonarBusy, setSonarBusy] = useState<string | null>(null);
  const [scanningProjectKey, setScanningProjectKey] = useState<string | null>(null);
  const [sonarLog, setSonarLog] = useState<string[]>([]);
  const [tokenValid, setTokenValid] = useState<boolean | null>(null);
  const [excludeRepos, setExcludeRepos] = useState<Set<string>>(new Set());
  const evaluatingRef = useRef(false);
  const lastRunRef = useRef<LastRunLog | null>(null);

  const qg = report?.quality_gate;
  const passed = qg?.passed;
  const testCount = report?.tia?.tests.length ?? 0;
  const changedCount = report?.changed_files?.length ?? 0;

  const containerRunning = sonarDiscovery?.container?.running === true;
  const containerExists = !!sonarDiscovery?.container;
  const sonarReachable = sonarDiscovery?.reachable === true;
  const canInstallSonar = !containerExists && !sonarBusy;
  const canStartSonar = containerExists && !containerRunning && !sonarBusy;
  const repoProjects = Array.isArray(sonarSetup?.repo_projects) ? sonarSetup.repo_projects : [];
  const readyRepos = repoProjects.filter((p) => p.exists).length;
  const needsSonarSetup =
    sonarReachable &&
    (repoProjects.length > 0
      ? readyRepos < repoProjects.length
      : !sonarSetup?.project_exists || tokenValid === false || tokenValid === null);
  const canScanAll =
    sonarReachable &&
    tokenValid === true &&
    !sonarBusy &&
    !busy &&
    (repoProjects.length > 0 || sonarSetup?.project_exists === true);

  const refreshSonar = useCallback(async () => {
    const d = await discoverSonar();
    setSonarDiscovery(d.discovery);
    if (d.setup) setSonarSetup(d.setup);
    const token = await validateSonarToken().catch(() => null);
    setTokenValid(token?.valid ?? null);
  }, []);

  usePageContext(
    'Command Center',
    qg ? (passed ? 'Quality gate passed' : 'Quality gate failed') : undefined,
  );

  function setLastRunState(log: LastRunLog | null) {
    lastRunRef.current = log;
    setLastRun(log);
  }

  function syncPipelineFromLog(
    log: LastRunLog | null | undefined,
    evaluating: boolean,
    sseLiveStep: string | null = null,
  ) {
    if (!log || (!evaluating && !isEvaluationInProgress(log))) return;
    const { liveStep, liveSteps, liveSonarKey, sonarProjects, sonarPhase: phase } =
      mergePipelineFromLog(log, sseLiveStep);
    setLiveStep(liveStep);
    setLiveSonarKey(liveSonarKey);
    setLiveSteps(liveSteps);
    setSonarPhase(phase);
    if (sonarProjects.size > 0) {
      setSonarProjectSteps(Array.from(sonarProjects.values()));
    } else if (
      liveStep === 'sonar' &&
      repoProjects.length > 0 &&
      (phase === 'preparing' || phase === 'scanning')
    ) {
      setSonarProjectSteps(
        Array.from(seedSonarProjects(repoProjects, new Map()).values()),
      );
    }
    setBusy('evaluate');
    evaluatingRef.current = true;
  }

  useEffect(() => {
    function resyncStatus() {
      fetchShipStatus()
        .then((d) => {
          setBranch(d.branch ?? null);
          setGitRoots(d.config?.ship?.git_roots ?? []);
          setExcludeRepos(new Set(d.config?.sonar?.exclude_repos ?? []));
          const inProgress = !!d.evaluating || isEvaluationInProgress(d.last_run);
          if (d.report && !inProgress) setReport(d.report);
          if (d.last_run) {
            setLastRunState(d.last_run);
            if (inProgress) {
              syncPipelineFromLog(d.last_run, true);
            } else {
              const parsed = parsePipelineFromLog(d.last_run);
              if (parsed.liveSteps.size > 0) setLiveSteps(parsed.liveSteps);
              if (parsed.sonarProjects.size > 0) {
                setSonarProjectSteps(Array.from(parsed.sonarProjects.values()));
              }
            }
          } else if (d.evaluating) {
            setBusy('evaluate');
            evaluatingRef.current = true;
          }
        })
        .catch(() => {});
    }
    resyncStatus();
    refreshSonar().catch(() => {});

    // Shared SSE hub (one connection per URL) with auto-reconnect + status resync.
    let disposed = false;
    let wasConnected = true;

    const handleShipEvent = (ev: MessageEvent) => {
      try {
        const payload = JSON.parse(ev.data);
        if (payload.type === 'step_started') {
          setLiveStep(payload.step ?? null);
          if (payload.step === 'sonar' && repoProjects.length > 0) {
            setSonarProjectSteps(
              Array.from(
                seedSonarProjects(repoProjects, new Map()).values(),
              ),
            );
          }
          if (payload.step) evaluatingRef.current = true;
        }
        if (payload.type === 'sonar_project_started') {
          setLiveStep('sonar');
          setLiveSonarKey(payload.project_key ?? null);
          setSonarProjectSteps((prev) =>
            Array.from(
              applySonarProjectEvent(
                new Map(asSonarProjectSteps(prev).map((p) => [p.key, p])),
                {
                  type: 'sonar_project_started',
                  project_key: payload.project_key,
                  repo_name: payload.repo_name,
                },
              ).values(),
            ),
          );
        }
        if (payload.type === 'sonar_project_finished') {
          setLiveSonarKey((prev) =>
            prev === payload.project_key ? null : prev,
          );
          setSonarProjectSteps((prev) =>
            Array.from(
              applySonarProjectEvent(
                new Map(asSonarProjectSteps(prev).map((p) => [p.key, p])),
                {
                  type: 'sonar_project_finished',
                  project_key: payload.project_key,
                  repo_name: payload.repo_name,
                  ok: payload.ok,
                },
              ).values(),
            ),
          );
        }
        if (payload.type === 'sonar_project_skipped') {
          setSonarProjectSteps((prev) =>
            Array.from(
              applySonarProjectEvent(
                new Map(asSonarProjectSteps(prev).map((p) => [p.key, p])),
                {
                  type: 'sonar_project_skipped',
                  project_key: payload.project_key,
                  repo_name: payload.repo_name,
                  reason: payload.reason,
                },
              ).values(),
            ),
          );
        }
        if (payload.type === 'step_finished') {
          setLiveStep((prev) => (prev === payload.step ? null : prev));
          if (payload.step === 'sonar') {
            setLiveSonarKey(null);
            setSonarProjectSteps((prev) => finalizeSonarProjectSteps(prev));
          }
          if (payload.step === 'sonar' && !payload.ok) {
            setSonarProjectSteps((prev) =>
              asSonarProjectSteps(prev).map((p) =>
                p.status === 'pending' || p.status === 'active'
                  ? { ...p, status: 'skipped' as const }
                  : p,
              ),
            );
          }
          if (payload.step) {
            setLiveSteps((prev) => {
              const next = new Map(prev);
              next.set(payload.step, {
                step: payload.step,
                status: payload.ok ? 'passed' : 'failed',
                detail: payload.detail ?? undefined,
              });
              return next;
            });
          }
        }
        if (payload.type === 'run_log_updated' && payload.last_run) {
          setLastRunState(payload.last_run);
          if (isEvaluationInProgress(payload.last_run)) {
            setLiveStep((prev) => {
              const { liveStep, liveSteps, liveSonarKey, sonarProjects, sonarPhase: phase } =
                mergePipelineFromLog(payload.last_run, prev);
              setLiveSteps(liveSteps);
              setLiveSonarKey(liveSonarKey);
              setSonarPhase(phase);
              if (sonarProjects.size > 0) {
                setSonarProjectSteps(Array.from(sonarProjects.values()));
              } else if (
                liveStep === 'sonar' &&
                repoProjects.length > 0 &&
                (phase === 'preparing' || phase === 'scanning')
              ) {
                setSonarProjectSteps(
                  Array.from(seedSonarProjects(repoProjects, new Map()).values()),
                );
              }
              setBusy('evaluate');
              evaluatingRef.current = true;
              return liveStep;
            });
          } else if (evaluatingRef.current) {
            evaluatingRef.current = false;
            setBusy(null);
            setLiveStep(null);
            setLiveSonarKey(null);
            setSonarPhase(null);
          }
        }
        if (payload.type === 'report_updated' && payload.report) {
          const inProgress =
            evaluatingRef.current && isEvaluationInProgress(lastRunRef.current);
          setReport(payload.report);
          if (!inProgress) {
            evaluatingRef.current = false;
            setLiveSteps(new Map());
            setBusy(null);
            setLiveStep(null);
            setLiveSonarKey(null);
            setMsg('Evaluation complete');
          }
        }
        if (payload.type === 'error') {
          setErr(payload.message ?? 'Evaluation failed');
          evaluatingRef.current = false;
          setBusy(null);
          setLiveStep(null);
        }
        if (payload.type === 'git_changed' && payload.branch) setBranch(payload.branch);
      } catch {
        /* ignore */
      }
    };

    const unsub = subscribeSharedEventSource('/api/ship/events', {
      events: { message: handleShipEvent },
      onOpen: () => {
        if (disposed) return;
        setConnected(true);
        if (!wasConnected) resyncStatus();
        wasConnected = true;
      },
      onError: () => {
        if (disposed) return;
        setConnected(false);
        wasConnected = false;
      },
    });

    return () => {
      disposed = true;
      unsub();
    };
  }, [refreshSonar, repoProjects]);

  async function scanProject(projectKey: string, projectName: string) {
    setScanningProjectKey(projectKey);
    setSonarBusy(`scan:${projectKey}`);
    setSonarLog([]);
    setErr(null);
    setLiveStep('sonar');
    setLiveSonarKey(projectKey);
    setSonarProjectSteps((prev) => {
      const steps = asSonarProjectSteps(prev);
      const next = steps.length > 0 ? steps.map((p) =>
        p.key === projectKey ? { ...p, status: 'active' as const } : p,
      ) : [{ key: projectKey, name: projectName, status: 'active' as const }];
      return next;
    });
    const append = (line: string) => setSonarLog((prev) => [...prev, line]);
    try {
      await streamSonarScanProject(projectKey, (ev) => {
        if (ev.type === 'log') append(ev.line);
        if (ev.type === 'done') {
          if (ev.logs?.length) setSonarLog(ev.logs);
          if (!ev.ok) throw new Error(ev.error ?? 'Scan failed');
          setMsg(`Scan complete — ${projectName}`);
        }
      });
      setSonarProjectSteps((prev) =>
        asSonarProjectSteps(prev).map((p) => p.key === projectKey ? { ...p, status: 'passed' as const } : p),
      );
      await refreshSonar();
    } catch (e) {
      setErr(String(e));
      setSonarProjectSteps((prev) =>
        asSonarProjectSteps(prev).map((p) => p.key === projectKey ? { ...p, status: 'failed' as const } : p),
      );
    } finally {
      setScanningProjectKey(null);
      setSonarBusy(null);
      setLiveSonarKey(null);
      setLiveStep(null);
    }
  }

  async function runSonarStream(
    label: string,
    stream: (onEvent: (ev: SonarStreamEvent) => void, signal?: AbortSignal) => Promise<void>,
  ) {
    setSonarBusy(label);
    setSonarLog([]);
    if (label === 'scan') {
      setLiveStep('sonar');
      setSonarProjectSteps(
        repoProjects.map((p) => ({ key: p.key, name: p.name, status: 'pending' as const })),
      );
    }
    const append = (line: string) => {
      setSonarLog((prev) => [...prev, line]);
      if (label === 'scan') {
        const startMatch = line.match(/\[\d+\/\d+\] Scanning .+? \(([\w.-]+)\)/);
        if (startMatch) {
          setLiveSonarKey(startMatch[1]);
          setSonarProjectSteps((prev) =>
            asSonarProjectSteps(prev).map((p) => p.key === startMatch[1] ? { ...p, status: 'active' as const } : p),
          );
        }
        const doneMatch = line.match(/✓ ([\w.-]+) complete/);
        if (doneMatch) {
          setLiveSonarKey((prev) => prev === doneMatch[1] ? null : prev);
          setSonarProjectSteps((prev) =>
            asSonarProjectSteps(prev).map((p) => p.key === doneMatch[1] ? { ...p, status: 'passed' as const } : p),
          );
        }
        const failMatch = line.match(/✕ ([\w.-]+)/);
        if (failMatch) {
          setLiveSonarKey((prev) => prev === failMatch[1] ? null : prev);
          setSonarProjectSteps((prev) =>
            asSonarProjectSteps(prev).map((p) => p.key === failMatch[1] ? { ...p, status: 'failed' as const } : p),
          );
        }
        const skipMatch = line.match(/– (.+?) skipped/);
        if (skipMatch) {
          const skippedName = skipMatch[1];
          setSonarProjectSteps((prev) =>
            asSonarProjectSteps(prev).map((p) =>
              p.name === skippedName || p.key.endsWith(`-${skippedName}`)
                ? { ...p, status: 'skipped' as const }
                : p,
            ),
          );
        }
      }
    };
    try {
      await stream((ev) => {
        if (ev.type === 'log') append(ev.line);
        if (ev.type === 'done') {
          if (ev.logs?.length) setSonarLog(ev.logs);
          if (!ev.ok) throw new Error(ev.error ?? 'SonarQube operation failed');
          if (label === 'scan' && ev.quality_gate) {
            setMsg(
              ev.quality_gate.passed
                ? `All projects scanned — quality gate ${ev.quality_gate.status}`
                : `Scan finished — quality gate ${ev.quality_gate.status}`,
            );
          }
        }
      });
      await refreshSonar();
      if (label === 'install') setMsg('SonarQube is live');
      else if (label === 'start') setMsg('SonarQube started');
    } catch (e) {
      setErr(String(e));
    } finally {
      setSonarBusy(null);
      if (label === 'scan') {
        setLiveStep(null);
        setLiveSonarKey(null);
        setSonarProjectSteps((prev) => finalizeSonarProjectSteps(prev));
      }
    }
  }

  async function setupSonar() {
    setSonarBusy('bootstrap');
    setErr(null);
    try {
      const r = await bootstrapSonar();
      if (!r.ok) throw new Error(r.error ?? 'SonarQube setup failed');
      if (r.setup) setSonarSetup(r.setup);
      await refreshSonar();
      setMsg(r.result?.message ?? 'SonarQube project and token configured');
    } catch (e) {
      setErr(String(e));
    } finally {
      setSonarBusy(null);
    }
  }

  async function regenerateToken() {
    setSonarBusy('token');
    setErr(null);
    try {
      const r = await regenerateSonarToken();
      if (!r.ok) throw new Error(r.error ?? 'Token regeneration failed');
      if (r.setup) setSonarSetup(r.setup);
      await refreshSonar();
      setMsg('Scanner token regenerated');
    } catch (e) {
      setErr(String(e));
    } finally {
      setSonarBusy(null);
    }
  }

  async function toggleExclude(repoName: string, exclude: boolean) {
    setErr(null);
    try {
      const r = await toggleSonarExclude(repoName, exclude);
      if (!r.ok) throw new Error('Failed to update exclusion');
      setExcludeRepos(new Set(r.exclude_repos));
      setMsg(exclude ? `${repoName} excluded from scans` : `${repoName} included in scans`);
    } catch (e) {
      setErr(String(e));
    }
  }

  async function runCommand(cmd: string) {
    setErr(null);
    setMsg(null);
    if (cmd === 'evaluate') {
      setBusy('evaluate');
      evaluatingRef.current = true;
      setLiveSteps(new Map());
      setLiveStep(null);
      setLiveSonarKey(null);
      if (repoProjects.length > 0) {
        setSonarProjectSteps(
          repoProjects.map((p) => ({
            key: p.key,
            name: p.name,
            status: 'pending' as const,
          })),
        );
      } else {
        setSonarProjectSteps([]);
      }
      setLastRunState({ ok: true, lines: [], started_at: new Date().toISOString() });
      try {
        const r = await runShipCommand(cmd);
        if (r.last_run) {
          setLastRunState(r.last_run);
          syncPipelineFromLog(r.last_run, !!r.evaluating || !!r.started);
        }
        if (!r.ok && !r.started) throw new Error(r.error ?? 'Command failed');
        if (r.report) {
          setReport(r.report);
          evaluatingRef.current = false;
          setBusy(null);
          setMsg('Evaluation complete');
        }
      } catch (e) {
        setErr(String(e));
        evaluatingRef.current = false;
        setBusy(null);
      }
      return;
    }
    setBusy(cmd);
    try {
      const r = await runShipCommand(cmd);
      if (r.last_run) setLastRun(r.last_run);
      if (!r.ok) throw new Error(r.error ?? 'Command failed');
      if (r.report) setReport(r.report);
      if (r.pr) setMsg(`Draft PR #${r.pr.number} — ${r.pr.url}`);
      else setMsg('Done');
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  }

  const stepsByName = new Map<string, GateStep>();
  for (const s of qg?.steps ?? []) {
    stepsByName.set(s.step, s);
  }
  for (const [name, step] of liveSteps) {
    if (!stepsByName.has(name)) {
      stepsByName.set(name, step);
    }
  }

  const sonarEvalActive =
    busy === 'evaluate' || isEvaluationInProgress(lastRun) || liveStep === 'sonar';
  const pipelineSonarProjects =
    sonarProjectSteps.length > 0
      ? sonarProjectSteps
      : sonarEvalActive && repoProjects.length > 0
        ? repoProjects.map((p) => ({
            key: p.key,
            name: p.name,
            status: 'pending' as const,
          }))
        : [];

  const sonarStepsByKey = new Map(pipelineSonarProjects.map((p) => [p.key, p]));

  return (
    <PageShell>
      <PageHero
        title="Command Center"
        subtitle="Git-aware quality gate with live pipeline updates via SSE."
        actions={
          <>
            <button type="button" className="btn" disabled={!!busy} onClick={onOpenSonar}>
              SonarQube
            </button>
            <button type="button" className="btn primary" disabled={!!busy} onClick={() => runCommand('evaluate')}>
              {busy === 'evaluate' ? <BusyLabel label="Evaluating…" /> : 'Evaluate'}
            </button>
            {canScanAll && (
              <button
                type="button"
                className="btn"
                disabled={!!sonarBusy || !!busy}
                onClick={() => runSonarStream('scan', streamSonarScanAll)}
              >
                {sonarBusy === 'scan' ? <BusyLabel label="Scanning…" /> : 'Scan all'}
              </button>
            )}
            <button type="button" className="btn" disabled={!!busy} onClick={() => runCommand('draft')}>
              {busy === 'draft' ? <BusyLabel label="Creating…" /> : 'Draft PR'}
            </button>
          </>
        }
      />

      <PageToasts ok={msg} err={err} />

      <PageStack>
        <PageCard title="Overview" description="Current branch and pipeline status.">
          <StatusPanel title="Metrics" className="settings-status-grid--metrics">
            <StatusPill label="Branch" value={branch ?? '—'} truncate />
            <StatusPill
              label="Git repos"
              value={gitRoots.length ? String(gitRoots.length) : '—'}
              tone={gitRoots.length ? 'ok' : 'warn'}
            />
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
              className={`ship-gate${passed === true ? ' ship-gate--pass' : passed === false ? ' ship-gate--fail' : ''}${busy === 'evaluate' || isEvaluationInProgress(lastRun) || liveStep ? ' ship-gate--live' : ''}`}
              style={{ margin: '0 clamp(16px, 2vw, 28px) 14px', borderRadius: 8 }}
            >
              <div className="ship-gate-status">
                <span className="ship-gate-label">Status</span>
                <strong className="ship-gate-title">
                  {!qg && (busy === 'evaluate' || isEvaluationInProgress(lastRun))
                    ? 'Evaluating…'
                    : !qg
                      ? 'Not evaluated yet'
                      : passed
                        ? 'All checks passed'
                        : 'Checks failed'}
                </strong>
                {liveStep && <span className="badge ship-live-badge">Running {liveStep}</span>}
              </div>
            </div>
          </PageCardBody>
        </PageCard>

        <PageCard title="Pipeline" description="Step-by-step quality gate progress.">
          <PipelineTrack
            stepsByName={stepsByName}
            liveStep={liveStep}
            liveSonarKey={liveSonarKey}
            sonarPhase={sonarPhase}
            sonarProjects={pipelineSonarProjects}
          />
        </PageCard>

        <PageCard
          title="SonarQube"
          description="Install, start, and configure SonarQube for the quality gate."
        >
          <PageCardBody>
            <StatusPanel title="Sonar status">
              <StatusPill
                label="API"
                value={sonarReachable ? 'reachable' : 'offline'}
                tone={sonarReachable ? 'ok' : 'warn'}
              />
              <StatusPill
                label="Container"
                value={
                  sonarDiscovery?.container
                    ? sonarDiscovery.container.running
                      ? 'running'
                      : 'stopped'
                    : 'not installed'
                }
                tone={containerRunning ? 'ok' : 'warn'}
              />
              <StatusPill
                label="Database"
                value={
                  sonarDiscovery?.embedded_database
                    ? 'embedded H2'
                    : sonarDiscovery?.database
                      ? sonarDiscovery.database.running
                        ? 'PostgreSQL'
                        : 'postgres stopped'
                      : sonarDiscovery?.container
                        ? 'unknown'
                        : '—'
                }
                tone={
                  sonarDiscovery?.embedded_database
                    ? 'warn'
                    : sonarDiscovery?.database?.running
                      ? 'ok'
                      : 'warn'
                }
              />
              <StatusPill
                label="Project"
                value={
                  repoProjects.length
                    ? `${readyRepos}/${repoProjects.length} projects`
                    : sonarSetup?.project_exists
                      ? 'ready'
                      : 'missing'
                }
                tone={
                  repoProjects.length
                    ? readyRepos === repoProjects.length
                      ? 'ok'
                      : 'warn'
                    : sonarSetup?.project_exists
                      ? 'ok'
                      : 'warn'
                }
              />
              <StatusPill
                label="Token"
                value={
                  tokenValid === true ? 'valid' : tokenValid === false ? 'invalid' : 'unknown'
                }
                tone={tokenValid === true ? 'ok' : 'warn'}
              />
            </StatusPanel>

            {/* Action buttons row */}
            <div className="sq-actions-bar">
              {canInstallSonar && (
                <button
                  type="button"
                  className="btn primary"
                  disabled={!!sonarBusy || !!busy}
                  onClick={() => runSonarStream('install', streamSonarInstall)}
                >
                  {sonarBusy === 'install' ? 'Installing…' : 'Install & start SonarQube'}
                </button>
              )}
              {canStartSonar && (
                <button
                  type="button"
                  className="btn primary"
                  disabled={!!sonarBusy || !!busy}
                  onClick={() => runSonarStream('start', streamSonarStart)}
                >
                  {sonarBusy === 'start' ? 'Starting…' : 'Start container'}
                </button>
              )}
              {needsSonarSetup && (
                <button
                  type="button"
                  className="btn primary"
                  disabled={!!sonarBusy || !!busy}
                  onClick={setupSonar}
                >
                  {sonarBusy === 'bootstrap' ? 'Setting up…' : 'Setup project & token'}
                </button>
              )}
              {sonarReachable && tokenValid === false && (
                <button
                  type="button"
                  className="btn"
                  disabled={!!sonarBusy || !!busy}
                  onClick={regenerateToken}
                >
                  {sonarBusy === 'token' ? 'Regenerating…' : 'Regenerate token'}
                </button>
              )}
              <button
                type="button"
                className="btn btn-subtle"
                disabled={!!sonarBusy}
                onClick={() => refreshSonar().catch(() => {})}
              >
                Refresh
              </button>
              <button type="button" className="btn btn-subtle" onClick={onOpenSonar}>
                SonarQube page
              </button>
            </div>

            {/* Project cards grid */}
            {repoProjects.length > 0 && (
              <div className="sq-project-section">
                <div className="sq-project-header">
                  <h3 className="sq-project-heading">
                    Projects ({repoProjects.length})
                  </h3>
                  {canScanAll && repoProjects.length > 1 && (
                    <button
                      type="button"
                      className="btn primary"
                      disabled={!!sonarBusy || !!busy}
                      onClick={() => runSonarStream('scan', streamSonarScanAll)}
                    >
                      {sonarBusy === 'scan'
                        ? 'Scanning all…'
                        : `Scan all ${repoProjects.length} projects`}
                    </button>
                  )}
                </div>

                {!sonarReachable && (
                  <div className="sq-project-alert sq-project-alert--warn">
                    SonarQube API is not reachable. Start the container or check the host configuration.
                  </div>
                )}
                {sonarReachable && tokenValid === false && (
                  <div className="sq-project-alert sq-project-alert--warn">
                    Scanner token is invalid. Click "Regenerate token" or "Setup project & token" to fix.
                  </div>
                )}
                {sonarReachable && tokenValid === null && (
                  <div className="sq-project-alert sq-project-alert--info">
                    Validating scanner token…
                  </div>
                )}

                <div className="sq-project-grid">
                  {repoProjects.map((p) => {
                    const isExcluded = excludeRepos.has(p.name);
                    const pipelineStep = sonarStepsByKey.get(p.key);
                    const cardStatus = isExcluded
                      ? { label: 'Excluded', variant: 'skipped' as const, scanning: false }
                      : resolveSonarProjectCardStatus(p.exists, pipelineStep, {
                          evalActive: sonarEvalActive,
                          liveSonarKey,
                          projectKey: p.key,
                        });
                    const isManualScan =
                      scanningProjectKey === p.key || sonarBusy === `scan:${p.key}`;
                    const isScanning = cardStatus.scanning || isManualScan;
                    const canScan = sonarReachable && tokenValid === true && p.exists && !sonarBusy && !busy && !isExcluded;
                    const blockedReason = isExcluded
                      ? 'Project excluded from scans'
                      : !sonarReachable
                        ? 'SonarQube offline'
                        : tokenValid !== true
                          ? 'Token not valid'
                          : !p.exists
                            ? 'Project not provisioned'
                            : sonarBusy
                              ? 'Another operation running'
                              : busy
                                ? 'Pipeline running'
                                : null;

                    return (
                      <div
                        key={p.key}
                        className={`sq-project-card${isScanning ? ' sq-project-card--scanning' : ''}${!p.exists ? ' sq-project-card--missing' : ''}${isExcluded ? ' sq-project-card--excluded' : ''}${cardStatus.variant === 'failed' ? ' sq-project-card--failed' : ''}`}
                      >
                        <div className="sq-project-card-top">
                          <div className="sq-project-card-info">
                            <span className="sq-project-card-name">{p.name}</span>
                            <span className="sq-project-card-key">{p.key}</span>
                          </div>
                          <span
                            className={`sq-project-card-status sq-project-card-status--${cardStatus.variant}`}
                          >
                            {cardStatus.label}
                          </span>
                        </div>

                        <div className="sq-project-card-actions">
                          {isScanning ? (
                            <button type="button" className="btn primary btn-sm" disabled>
                              <span className="sq-spinner" /> Scanning…
                            </button>
                          ) : (
                            <button
                              type="button"
                              className="btn primary btn-sm"
                              disabled={!canScan}
                              title={blockedReason ?? 'Run SonarQube analysis on this project'}
                              onClick={() => scanProject(p.key, p.name)}
                            >
                              Scan
                            </button>
                          )}
                          <button
                            type="button"
                            className={`btn btn-sm${isExcluded ? ' btn-subtle' : ' btn-danger-subtle'}`}
                            title={isExcluded ? 'Include this project in scans' : 'Exclude this project from scans'}
                            onClick={() => toggleExclude(p.name, !isExcluded)}
                          >
                            {isExcluded ? 'Include' : 'Exclude'}
                          </button>
                        </div>

                        {isScanning && sonarLog.length > 0 && (
                          <div className="sq-project-card-log">
                            <pre>{sonarLog.slice(-8).join('\n')}</pre>
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            )}

            {repoProjects.length === 0 && sonarReachable && (sonarSetup?.project_exists || canScanAll) && (
              <div className="sq-actions-bar">
                <button
                  type="button"
                  className="btn primary"
                  disabled={!!sonarBusy || !!busy}
                  onClick={() => runSonarStream('scan', streamSonarScanAll)}
                >
                  {sonarBusy === 'scan' ? 'Scanning…' : 'Scan project'}
                </button>
              </div>
            )}

            {sonarLog.length > 0 && !scanningProjectKey && (
              <div className="settings-log-panel" style={{ margin: '0 clamp(16px, 2vw, 28px) 14px' }}>
                <div className="settings-log-header">
                  <span>Sonar log</span>
                  {sonarBusy && <span className="settings-log-live">live</span>}
                </div>
                <pre className="settings-log-body">{sonarLog.join('\n')}</pre>
              </div>
            )}
          </PageCardBody>
        </PageCard>

        <RunLogPanel
          log={lastRun}
          active={busy === 'evaluate' || !!liveStep}
          liveStep={liveStep}
          liveSonarKey={liveSonarKey}
          sonarProjects={sonarProjectSteps}
        />

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
