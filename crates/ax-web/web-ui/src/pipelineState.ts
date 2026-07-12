import type { GateStep, LastRunLog } from './shipApi';

const VISIBLE_STEPS = new Set(['index', 'tia', 'tests', 'sonar', 'policy']);

export type SonarProjectStatus = 'pending' | 'active' | 'passed' | 'failed' | 'skipped';

export interface SonarProjectStep {
  key: string;
  name: string;
  status: SonarProjectStatus;
}

export function isEvaluationInProgress(log: LastRunLog | null | undefined): boolean {
  return !!(log?.started_at && !log?.finished_at);
}

export function parsePipelineFromLog(log: LastRunLog | null | undefined): {
  liveStep: string | null;
  liveSteps: Map<string, GateStep>;
  liveSonarKey: string | null;
  sonarProjects: Map<string, SonarProjectStep>;
} {
  const liveSteps = new Map<string, GateStep>();
  const sonarProjects = new Map<string, SonarProjectStep>();
  let liveStep: string | null = null;
  let liveSonarKey: string | null = null;

  if (!log?.lines?.length) {
    return { liveStep, liveSteps, liveSonarKey, sonarProjects };
  }

  for (const line of log.lines) {
    const sonarStart = line.match(/▶ sonar:([\w.-]+) — (.+?) \(\d+\/\d+\)/);
    if (sonarStart) {
      liveStep = 'sonar';
      liveSonarKey = sonarStart[1];
      sonarProjects.set(sonarStart[1], {
        key: sonarStart[1],
        name: sonarStart[2],
        status: 'active',
      });
      continue;
    }

    const sonarOk = line.match(/✓ sonar:([\w.-]+) — (.+)/);
    if (sonarOk) {
      sonarProjects.set(sonarOk[1], {
        key: sonarOk[1],
        name: sonarOk[2],
        status: 'passed',
      });
      if (liveSonarKey === sonarOk[1]) liveSonarKey = null;
      continue;
    }

    const sonarFail = line.match(/✕ sonar:([\w.-]+) — (.+?):/);
    if (sonarFail) {
      sonarProjects.set(sonarFail[1], {
        key: sonarFail[1],
        name: sonarFail[2],
        status: 'failed',
      });
      if (liveSonarKey === sonarFail[1]) liveSonarKey = null;
      continue;
    }

    const sonarSkip = line.match(/– sonar:([\w.-]+) — (.+?) — skipped \((.+)\)/);
    if (sonarSkip) {
      sonarProjects.set(sonarSkip[1], {
        key: sonarSkip[1],
        name: sonarSkip[2],
        status: 'skipped',
      });
      if (liveSonarKey === sonarSkip[1]) liveSonarKey = null;
      continue;
    }

    const startMatch = line.match(/▶ (\w+)/);
    if (startMatch && VISIBLE_STEPS.has(startMatch[1])) {
      liveStep = startMatch[1];
      if (startMatch[1] !== 'sonar') liveSonarKey = null;
      continue;
    }

    const okMatch = line.match(/✓ (\w+)(?: — (.+))?/);
    if (okMatch && VISIBLE_STEPS.has(okMatch[1])) {
      const step = okMatch[1];
      const detail = okMatch[2];
      const lower = detail?.toLowerCase() ?? '';
      const status =
        lower.includes('disabled') || lower.includes('skipped') ? 'skipped' : 'passed';
      liveSteps.set(step, { step, status, detail });
      if (liveStep === step) liveStep = null;
      if (step !== 'sonar') liveSonarKey = null;
      continue;
    }

    const failMatch = line.match(/✕ (\w+)(?: — (.+))?/);
    if (failMatch && VISIBLE_STEPS.has(failMatch[1])) {
      const step = failMatch[1];
      liveSteps.set(step, { step, status: 'failed', detail: failMatch[2] });
      if (liveStep === step) liveStep = null;
      if (step !== 'sonar') liveSonarKey = null;
    }
  }

  if (!isEvaluationInProgress(log)) {
    liveStep = null;
    liveSonarKey = null;
  }

  return { liveStep, liveSteps, liveSonarKey, sonarProjects };
}

export function mergePipelineFromLog(
  log: LastRunLog | null | undefined,
  sseLiveStep: string | null,
): {
  liveStep: string | null;
  liveSteps: Map<string, GateStep>;
  liveSonarKey: string | null;
  sonarProjects: Map<string, SonarProjectStep>;
} {
  const parsed = parsePipelineFromLog(log);
  if (!isEvaluationInProgress(log)) {
    return parsed;
  }
  const liveStep =
    sseLiveStep && !parsed.liveSteps.has(sseLiveStep) ? sseLiveStep : parsed.liveStep;
  return { ...parsed, liveStep };
}

export function seedSonarProjects(
  repoProjects: Array<{ key: string; name: string }>,
  existing: Map<string, SonarProjectStep>,
): Map<string, SonarProjectStep> {
  const next = new Map(existing);
  for (const p of repoProjects) {
    if (!next.has(p.key)) {
      next.set(p.key, { key: p.key, name: p.name, status: 'pending' });
    }
  }
  return next;
}

export function applySonarProjectEvent(
  projects: Map<string, SonarProjectStep>,
  event:
    | { type: 'sonar_project_started'; project_key: string; repo_name: string }
    | { type: 'sonar_project_finished'; project_key: string; repo_name: string; ok: boolean }
    | { type: 'sonar_project_skipped'; project_key: string; repo_name: string; reason?: string },
): Map<string, SonarProjectStep> {
  const next = new Map(projects);
  if (event.type === 'sonar_project_started') {
    next.set(event.project_key, {
      key: event.project_key,
      name: event.repo_name,
      status: 'active',
    });
  } else if (event.type === 'sonar_project_skipped') {
    next.set(event.project_key, {
      key: event.project_key,
      name: event.repo_name,
      status: 'skipped',
    });
  } else {
    next.set(event.project_key, {
      key: event.project_key,
      name: event.repo_name,
      status: event.ok ? 'passed' : 'failed',
    });
  }
  return next;
}

export function finalizeSonarProjectSteps(
  projects: SonarProjectStep[],
): SonarProjectStep[] {
  return projects.map((p) =>
    p.status === 'pending' ? { ...p, status: 'skipped' as const } : p,
  );
}
