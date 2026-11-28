import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import {
  applySonarProjectEvent,
  finalizeSonarProjectSteps,
  mergePipelineFromLog,
  parsePipelineFromLog,
  resolveSonarProjectCardStatus,
  RUN_LOG_PATTERNS,
  seedSonarProjects,
} from './pipelineState.ts';

/** Lines in the exact shape ax-ship's RunLogger writes (`[timestamp] message`). */
const at = (msg: string) => `[2026-11-27 10:00:00] ${msg}`;

const RUNNING = { started_at: '2026-11-27 10:00:00', finished_at: null, ok: true };
const DONE = { started_at: '2026-11-27 10:00:00', finished_at: '2026-11-27 10:01:00', ok: true };

test('an empty or missing log has no live state', () => {
  for (const log of [null, undefined, { ...RUNNING, lines: [] }]) {
    const parsed = parsePipelineFromLog(log);
    assert.equal(parsed.liveStep, null);
    assert.equal(parsed.liveSteps.size, 0);
    assert.equal(parsed.sonarPhase, null);
  }
});

const STEP_CASES: Array<{ name: string; lines: string[]; liveStep: string | null; steps: Record<string, [string, string | undefined]> }> = [
  { name: 'a started step is live', lines: [at('▶ index')], liveStep: 'index', steps: {} },
  { name: 'a passed step is no longer live', lines: [at('▶ index'), at('✓ index — 1615 files')], liveStep: null, steps: { index: ['passed', '1615 files'] } },
  { name: 'a step without detail passes', lines: [at('▶ tia'), at('✓ tia')], liveStep: null, steps: { tia: ['passed', undefined] } },
  { name: 'disabled and skipped details mean skipped', lines: [at('✓ tests — disabled in config'), at('✓ policy — Skipped: no rules')], liveStep: null, steps: { tests: ['skipped', 'disabled in config'], policy: ['skipped', 'Skipped: no rules'] } },
  { name: 'a failed step records its detail', lines: [at('▶ tests'), at('✕ tests — 3 failed')], liveStep: null, steps: { tests: ['failed', '3 failed'] } },
  { name: 'steps outside the visible set are ignored', lines: [at('▶ diff'), at('✓ diff — 4 files')], liveStep: null, steps: {} },
  { name: 'the next step takes over as live', lines: [at('▶ index'), at('✓ index'), at('▶ tia')], liveStep: 'tia', steps: { index: ['passed', undefined] } },
];

for (const c of STEP_CASES) {
  test(`parsePipelineFromLog: ${c.name}`, () => {
    const parsed = parsePipelineFromLog({ ...RUNNING, lines: c.lines });
    assert.equal(parsed.liveStep, c.liveStep);
    const steps = Object.fromEntries([...parsed.liveSteps].map(([k, v]) => [k, [v.status, v.detail]]));
    assert.deepEqual(steps, c.steps);
  });
}

test('sonar preparation, per-project scans and outcomes', () => {
  const lines = [
    at('Preparing SonarQube — 3 git repositories (projects + token)'),
    at('SonarQube scan — all 3 git repositories (2 changed file(s) in diff)'),
    at('▶ sonar:org.ax-core — ax-core (1/3)'),
    at('✓ sonar:org.ax-core — ax-core'),
    at('▶ sonar:org.ax-web — ax-web (2/3)'),
    at('✕ sonar:org.ax-web — ax-web: scanner exited with 1'),
    at('– sonar:org.ax-docs — ax-docs — skipped (no sources)'),
    at('▶ sonar:org.ax-cli — ax-cli (3/3)'),
  ];
  const parsed = parsePipelineFromLog({ ...RUNNING, lines });
  assert.equal(parsed.liveStep, 'sonar');
  assert.equal(parsed.sonarPhase, 'scanning');
  assert.equal(parsed.liveSonarKey, 'org.ax-cli');
  assert.deepEqual([...parsed.sonarProjects.values()], [
    { key: 'org.ax-core', name: 'ax-core', status: 'passed' },
    { key: 'org.ax-web', name: 'ax-web', status: 'failed' },
    { key: 'org.ax-docs', name: 'ax-docs', status: 'skipped' },
    { key: 'org.ax-cli', name: 'ax-cli', status: 'active' },
  ]);
});

test('a failed sonar project is recorded with or without a detail after the name', () => {
  for (const failLine of [
    '✕ sonar:org.api — api: scanner exited with 1',
    '✕ sonar:org.api — api:',
    '✕ sonar:org.api — api',
  ]) {
    const parsed = parsePipelineFromLog({ ...RUNNING, lines: [at('▶ sonar:org.api — api (1/2)'), at(failLine)] });
    assert.deepEqual(parsed.sonarProjects.get('org.api'), { key: 'org.api', name: 'api', status: 'failed' }, failLine);
    assert.equal(parsed.liveSonarKey, null, failLine);
  }
});

test('a later visible step clears the live sonar project', () => {
  const parsed = parsePipelineFromLog({ ...RUNNING, lines: [at('▶ sonar:org.api — api (1/1)'), at('▶ policy')] });
  assert.equal(parsed.liveStep, 'policy');
  assert.equal(parsed.liveSonarKey, null);
});

test('while SonarQube is being prepared the phase is preparing', () => {
  const parsed = parsePipelineFromLog({ ...RUNNING, lines: [at('▶ index'), at('✓ index'), at('Preparing SonarQube — 1 git repository (projects + token)')] });
  assert.equal(parsed.liveStep, 'sonar');
  assert.equal(parsed.sonarPhase, 'preparing');
});

test('a finished run keeps step outcomes but nothing is live', () => {
  const parsed = parsePipelineFromLog({ ...DONE, lines: [at('▶ index'), at('▶ sonar:k — repo (1/1)')] });
  assert.equal(parsed.liveStep, null);
  assert.equal(parsed.liveSonarKey, null);
  assert.equal(parsed.sonarPhase, null);
  assert.equal(parsed.sonarProjects.get('k')?.status, 'active');
});

test('mergePipelineFromLog prefers the SSE step unless the log already finished it', () => {
  const log = { ...RUNNING, lines: [at('▶ index'), at('✓ index')] };
  assert.equal(mergePipelineFromLog(log, 'tia').liveStep, 'tia');
  assert.equal(mergePipelineFromLog(log, 'index').liveStep, null);
  assert.equal(mergePipelineFromLog({ ...DONE, lines: [] }, 'tia').liveStep, null);
});

test('sonar project events, seeding and finalizing', () => {
  let projects = seedSonarProjects([{ key: 'a', name: 'A' }, { key: 'b', name: 'B' }], new Map());
  projects = applySonarProjectEvent(projects, { type: 'sonar_project_started', project_key: 'a', repo_name: 'A' });
  assert.equal(projects.get('a')?.status, 'active');
  projects = applySonarProjectEvent(projects, { type: 'sonar_project_finished', project_key: 'a', repo_name: 'A', ok: false });
  assert.equal(projects.get('a')?.status, 'failed');
  projects = seedSonarProjects([{ key: 'a', name: 'A' }], projects);
  assert.equal(projects.get('a')?.status, 'failed', 'seeding keeps an existing outcome');
  assert.deepEqual(finalizeSonarProjectSteps(projects).map((p) => p.status), ['failed', 'skipped']);
  assert.deepEqual(finalizeSonarProjectSteps(null), []);
});

test('resolveSonarProjectCardStatus', () => {
  const opts = { evalActive: true, liveSonarKey: null, projectKey: 'a' };
  assert.equal(resolveSonarProjectCardStatus(true, undefined, { ...opts, liveSonarKey: 'a' }).label, 'Scanning…');
  assert.equal(resolveSonarProjectCardStatus(true, { key: 'a', name: 'A', status: 'pending' }, opts).label, 'Queued');
  assert.equal(resolveSonarProjectCardStatus(true, { key: 'a', name: 'A', status: 'pending' }, { ...opts, evalActive: false }).label, 'Ready');
  assert.equal(resolveSonarProjectCardStatus(false, undefined, opts).label, 'Missing');
  assert.equal(resolveSonarProjectCardStatus(true, { key: 'a', name: 'A', status: 'passed' }, opts).variant, 'ok');
});

test('the run-log patterns match the lines the Rust writers format', () => {
  const runLog = readFileSync(new URL('../../../ax-ship/src/run_log.rs', import.meta.url), 'utf8');
  const pipeline = readFileSync(new URL('../../../ax-ship/src/pipeline.rs', import.meta.url), 'utf8');
  const values: Record<string, string> = {
    project_key: 'org.ax-web',
    repo_name: 'ax-web',
    index: '1',
    total: '2',
    detail: 'scanner exited with 1',
    reason: 'no sources',
    step: 'index',
    d: '12 files',
  };
  const render = (template: string) =>
    template.replace(/\{(\w*)\}/g, (_, name: string) => (name ? values[name] ?? name : '3'));
  const templates = [...runLog.matchAll(/format!\(\s*"([^"]+)"/g), ...pipeline.matchAll(/format!\(\s*"([^"]*SonarQube[^"]*)"/g)].map(
    (m) => render(m[1]),
  );
  const find = (prefix: string) => {
    const line = templates.find((t) => t.startsWith(prefix));
    assert.ok(line, `no Rust log line starts with ${prefix}`);
    return line;
  };
  assert.deepEqual(find('▶ sonar:').match(RUN_LOG_PATTERNS.sonarStart)?.slice(1, 3), ['org.ax-web', 'ax-web']);
  assert.deepEqual(find('✓ sonar:').match(RUN_LOG_PATTERNS.sonarOk)?.slice(1, 3), ['org.ax-web', 'ax-web']);
  assert.deepEqual(find('✕ sonar:').match(RUN_LOG_PATTERNS.sonarFail)?.slice(1, 3), ['org.ax-web', 'ax-web']);
  assert.deepEqual(find('– sonar:').match(RUN_LOG_PATTERNS.sonarSkip)?.slice(1, 3), ['org.ax-web', 'ax-web']);
  assert.match(find('Preparing SonarQube'), RUN_LOG_PATTERNS.sonarPreparing);
  assert.match(find('SonarQube scan'), RUN_LOG_PATTERNS.sonarScan);
  assert.match(find('Full SonarQube scan'), RUN_LOG_PATTERNS.sonarScan);
  assert.equal(find('▶ index').match(RUN_LOG_PATTERNS.stepStart)?.[1], 'index');
  assert.deepEqual(find('✓ index — ').match(RUN_LOG_PATTERNS.stepOk)?.slice(1, 3), ['index', '12 files']);
  assert.deepEqual(find('✕ index — ').match(RUN_LOG_PATTERNS.stepFail)?.slice(1, 3), ['index', 'scanner exited with 1']);
});
