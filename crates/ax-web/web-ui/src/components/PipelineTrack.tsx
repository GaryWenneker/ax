import type { GateStep } from '../shipApi';
import type { SonarProjectStep } from '../pipelineState';

const PIPELINE_STEPS = ['index', 'tia', 'tests', 'sonar', 'policy'] as const;

const STEP_LABELS: Record<(typeof PIPELINE_STEPS)[number], string> = {
  index: 'Index',
  tia: 'TIA',
  tests: 'Tests',
  sonar: 'Sonar',
  policy: 'Policy',
};

function stepIcon(status: string, active: boolean) {
  if (active) return '◌';
  if (status === 'passed') return '✓';
  if (status === 'failed') return '✕';
  if (status === 'skipped') return '–';
  return '·';
}

function sonarAggregateMeta(
  sonarProjects: SonarProjectStep[],
  liveSonarKey: string | null,
  active: boolean,
  sonarPhase: 'preparing' | 'scanning' | null = null,
) {
  if (sonarProjects.length === 0) return null;

  const total = sonarProjects.length;
  const scanning = sonarProjects.filter(
    (p) => p.status === 'active' || liveSonarKey === p.key,
  ).length;
  const skipped = sonarProjects.filter((p) => p.status === 'skipped').length;
  const done = sonarProjects.filter(
    (p) => p.status === 'passed' || p.status === 'failed' || p.status === 'skipped',
  ).length;
  const scanTotal = total - skipped;
  const scanDone = sonarProjects.filter(
    (p) => p.status === 'passed' || p.status === 'failed',
  ).length;

  if (active || scanning > 0) {
    const allPending = sonarProjects.every((p) => p.status === 'pending');
    const current = liveSonarKey
      ? sonarProjects.find((p) => p.key === liveSonarKey)
      : sonarProjects.find((p) => p.status === 'active');
    const progress = Math.min(
      scanDone + (scanning > 0 ? 1 : 0),
      Math.max(scanTotal, 1),
    );
    if (active && allPending) {
      if (sonarPhase === 'scanning') {
        return <span className="badge ship-live-badge">Starting scans…</span>;
      }
      return <span className="badge ship-live-badge">Preparing SonarQube…</span>;
    }
    if (active && scanning === 0 && !current && scanTotal > 0) {
      return <span className="badge ship-live-badge">Starting scans…</span>;
    }
    if (scanTotal === 0 && skipped > 0) {
      return <span className="badge ship-live-badge">{skipped} skipped — no changes</span>;
    }
    return (
      <span className="badge ship-live-badge">
        Scanning {progress}/{scanTotal || total}
        {current ? ` · ${current.name}` : ''}
        {skipped > 0 ? ` · ${skipped} skipped` : ''}
      </span>
    );
  }

  if (done === total) {
    const failed = sonarProjects.filter((p) => p.status === 'failed').length;
    return failed > 0 ? `${failed} failed · ${total} projects` : `${total} projects scanned`;
  }

  return `${total} project${total === 1 ? '' : 's'} queued`;
}

function stepMeta(
  step: GateStep | undefined,
  active: boolean,
  liveSonarKey: string | null,
  sonarProjects: SonarProjectStep[],
  stepName: (typeof PIPELINE_STEPS)[number],
  sonarPhase: 'preparing' | 'scanning' | null = null,
) {
  if (stepName === 'sonar' && sonarProjects.length > 0) {
    return sonarAggregateMeta(sonarProjects, liveSonarKey, active, sonarPhase);
  }
  if (active && liveSonarKey) {
    const current = sonarProjects.find((p) => p.key === liveSonarKey);
    return (
      <span className="badge ship-live-badge">{current?.name ?? liveSonarKey}</span>
    );
  }
  if (active) {
    return <span className="badge ship-live-badge">Running…</span>;
  }
  if (step?.detail) {
    return step.detail;
  }
  if (step?.status === 'pending') return 'Waiting';
  return step?.status ?? 'pending';
}

function sonarProjectIcon(status: SonarProjectStep['status'], active: boolean) {
  if (active || status === 'active') return '◌';
  if (status === 'passed') return '✓';
  if (status === 'failed') return '✕';
  if (status === 'skipped') return '–';
  return '·';
}

function sonarProjectStatusLabel(
  status: SonarProjectStep['status'],
  active: boolean,
  sonarStepActive: boolean,
  sonarPhase: 'preparing' | 'scanning' | null = null,
) {
  if (active || status === 'active') return 'Scanning…';
  if (status === 'passed') return 'Done';
  if (status === 'failed') return 'Failed';
  if (status === 'skipped') return 'Skipped';
  if (sonarStepActive && sonarPhase === 'preparing') return 'Queued';
  if (sonarStepActive) return 'Queued';
  return 'Queued';
}

interface Props {
  stepsByName: Map<string, GateStep>;
  liveStep: string | null;
  liveSonarKey?: string | null;
  sonarProjects?: SonarProjectStep[];
  sonarPhase?: 'preparing' | 'scanning' | null;
}

export default function PipelineTrack({
  stepsByName,
  liveStep,
  liveSonarKey = null,
  sonarProjects = [],
  sonarPhase = null,
}: Props) {
  const showSonarStack =
    sonarProjects.length > 0 &&
    (liveStep === 'sonar' ||
      sonarProjects.some((p) => p.status !== 'pending') ||
      stepsByName.get('sonar')?.status === 'passed' ||
      stepsByName.get('sonar')?.status === 'failed');

  return (
    <div className="ship-pipeline-wrap">
      <div className="ship-pipeline-track" role="list" aria-label="Quality gate pipeline">
        {PIPELINE_STEPS.map((name, index) => {
          const step = stepsByName.get(name);
          const active = liveStep === name || (name === 'sonar' && !!liveSonarKey);
          const rawStatus = active ? 'active' : (step?.status ?? 'pending');
          const status =
            rawStatus === 'passed' && step?.detail?.toLowerCase().includes('disabled')
              ? 'skipped'
              : rawStatus;
          const prev = index > 0 ? stepsByName.get(PIPELINE_STEPS[index - 1]) : null;
          const prevDone = index === 0 || prev?.status === 'passed' || prev?.status === 'skipped';
          const connectorActive = prevDone && (active || status === 'pending');

          return (
            <div
              key={name}
              className={`ship-pipeline-item${name === 'sonar' && showSonarStack ? ' ship-pipeline-item--sonar' : ''}`}
              role="listitem"
            >
              {index > 0 && (
                <div
                  className={`ship-pipeline-connector${connectorActive ? ' ship-pipeline-connector--flow' : ''}${prev?.status === 'passed' || prev?.status === 'skipped' ? ' ship-pipeline-connector--done' : ''}`}
                  aria-hidden="true"
                />
              )}
              <div
                className={`ship-pipeline-step ship-pipeline-step--${status}${active ? ' ship-pipeline-step--active' : ''}${name === 'sonar' && showSonarStack ? ' ship-pipeline-step--sonar-expanded' : ''}`}
              >
                <div className="ship-step-icon" aria-hidden="true">
                  <span className={active ? 'ship-step-icon--live' : undefined}>
                    {stepIcon(status, active)}
                  </span>
                </div>
                <div className="ship-step-body">
                  <div className="ship-step-name">{STEP_LABELS[name]}</div>
                  <div className="ship-step-meta muted">
                    {stepMeta(step, active, liveSonarKey, sonarProjects, name, sonarPhase)}
                  </div>

                  {name === 'sonar' && showSonarStack && (
                    <ul className="ship-pipeline-sonar-stack" aria-label="SonarQube scan projects">
                      {sonarProjects.map((project) => {
                        const projectActive =
                          liveSonarKey === project.key || project.status === 'active';
                        const projectStatus = projectActive ? 'active' : project.status;
                        const statusLabel = sonarProjectStatusLabel(
                          project.status,
                          projectActive,
                          active,
                          sonarPhase,
                        );

                        return (
                          <li
                            key={project.key}
                            className={`ship-pipeline-sonar-row ship-pipeline-sonar-row--${projectStatus}`}
                            title={project.key}
                          >
                            <span className="ship-pipeline-sonar-row-icon" aria-hidden="true">
                              <span className={projectActive ? 'ship-step-icon--live' : undefined}>
                                {sonarProjectIcon(project.status, projectActive)}
                              </span>
                            </span>
                            <span className="ship-pipeline-sonar-row-name">{project.name}</span>
                            <span
                              className={`ship-pipeline-sonar-row-status${projectActive ? ' ship-pipeline-sonar-row-status--live' : ''}`}
                              aria-live={projectActive ? 'polite' : undefined}
                            >
                              {statusLabel}
                            </span>
                          </li>
                        );
                      })}
                    </ul>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
