import { RUN_LOG_PATTERNS } from './pipelineState.ts';

/**
 * Normalize a leading unix timestamp in a log line to local date/time.
 * A value above 1e12 is milliseconds, a smaller one seconds; a stamp that
 * is not a valid date is left as it is.
 */
export function formatLogLine(line: string): string {
  return line.replace(/^\[(\d{10,})\]/, (match, raw: string) => {
    const n = Number(raw);
    const date = new Date(n > 1e12 ? n : n * 1000);
    if (Number.isNaN(date.getTime())) return match;
    return `[${date.toLocaleString()}]`;
  });
}

/** Human-readable label for what is currently running in the pipeline log. */
export function liveActivityLabel(
  lines: string[] | undefined,
  liveStep: string | null,
  liveSonarKey: string | null,
  sonarName?: string | null,
): string | null {
  if (liveSonarKey) {
    return sonarName ? `Sonar · ${sonarName}` : `Sonar · ${liveSonarKey}`;
  }
  if (liveStep) {
    const labels: Record<string, string> = {
      index: 'Indexing',
      diff: 'Diff',
      tia: 'Test impact',
      tests: 'Tests',
      sonar: 'Sonar',
      policy: 'Policy',
    };
    return labels[liveStep] ?? liveStep;
  }
  if (!lines?.length) return null;
  for (let i = lines.length - 1; i >= 0; i--) {
    const formatted = formatLogLine(lines[i]);
    const sonar = formatted.match(RUN_LOG_PATTERNS.sonarStart);
    if (sonar) return `Sonar · ${sonar[2]}`;
    const step = formatted.match(RUN_LOG_PATTERNS.stepStart);
    if (step) return step[1];
  }
  return null;
}
